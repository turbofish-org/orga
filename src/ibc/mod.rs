use std::convert::TryFrom;
use std::str::from_utf8;

#[cfg(feature = "abci")]
use crate::abci::{AbciQuery, BeginBlock};
use crate::call::Call;
use crate::client::Client;
use crate::coins::{Address, Amount};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
#[cfg(feature = "abci")]
use crate::plugins::BeginBlockCtx;
#[cfg(feature = "abci")]
use crate::plugins::Events;
use crate::plugins::Signer;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use client::ClientStore;
use encoding::*;
pub use ibc as ibc_rs;
use ibc::applications::transfer::msgs::transfer::MsgTransfer;
use ibc::applications::transfer::relay::send_transfer::send_transfer;
use ibc::core::ics02_client::height::Height;
use ibc::core::ics04_channel::timeout::TimeoutHeight;
use ibc::core::ics24_host::identifier::{ChannelId, PortId};
use ibc::core::ics26_routing::handler::dispatch;
use ibc::events::IbcEvent;
use ibc::handler::{HandlerOutput, HandlerOutputBuilder};
use ibc::signer::Signer as IbcSigner;
use ibc::timestamp::Timestamp;
pub use ibc_proto as proto;
use ibc_proto::cosmos::base::v1beta1::Coin;
use ibc_proto::ibc::core::channel::v1::PacketState;
use ics23::LeafOp;
use serde::{Deserialize, Serialize};
use tendermint_proto::abci::{EventAttribute, RequestQuery, ResponseQuery};
use tendermint_proto::Protobuf;

mod channel;
mod client;
mod connection;
pub mod encoding;
#[cfg(feature = "abci")]
mod grpc;
mod port;
mod routing;
mod transfer;

#[cfg(feature = "abci")]
pub use grpc::start_grpc;

use crate::store::Store;
use tendermint_proto::abci::Event;
use tendermint_proto::crypto::{ProofOp, ProofOps};

use self::channel::ChannelStore;
use self::connection::ConnectionStore;
use self::port::PortStore;
pub use self::routing::{IbcMessage, IbcTx};
use self::transfer::{Dynom, TransferModule};
use crate::describe::Describe;

#[derive(State, Call, Client, Query, Encode, Decode, Default, Serialize, Deserialize, Describe)]
pub struct Ibc {
    pub clients: ClientStore,
    pub connections: ConnectionStore,
    pub channels: ChannelStore,
    ports: PortStore,
    height: u64,
    #[call]
    pub transfers: TransferModule,
    pub(super) lunchbox: Lunchbox,
}

#[derive(Encode, Decode, Default, Serialize, Deserialize, Describe)]
pub struct Lunchbox(pub(super) Store);

impl State for Lunchbox {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.0 = unsafe { store.with_prefix(vec![]) };
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl From<Lunchbox> for () {
    fn from(_: Lunchbox) -> Self {}
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct TransferOpts {
    pub channel_id: Adapter<ChannelId>,
    pub port_id: Adapter<PortId>,
    pub amount: Amount,
    pub denom: Dynom,
    pub receiver: Adapter<IbcSigner>,
    pub timeout_height: Adapter<TimeoutHeight>,
    pub timeout_timestamp: Adapter<Timestamp>,
}

pub struct TransferArgs {
    pub channel_id: String,
    pub port_id: String,
    pub amount: Amount,
    pub denom: String,
    pub receiver: String,
}

impl TryFrom<TransferArgs> for TransferOpts {
    type Error = crate::Error;
    fn try_from(args: TransferArgs) -> crate::Result<Self> {
        let now_ns = Timestamp::now().nanoseconds();
        Ok(TransferOpts {
            channel_id: args
                .channel_id
                .parse::<ChannelId>()
                .map_err(|_| crate::Error::Ibc("Invalid channel id".into()))?
                .into(),
            port_id: args
                .port_id
                .parse::<PortId>()
                .map_err(|_| crate::Error::Ibc("Invalid port".into()))?
                .into(),
            amount: args.amount,
            denom: args.denom.as_str().parse().unwrap(),
            receiver: args
                .receiver
                .parse::<IbcSigner>()
                .map_err(|_| crate::Error::Ibc("Invalid receiver".into()))?
                .into(),
            timeout_height: TimeoutHeight::Never.into(),
            timeout_timestamp: Timestamp::from_nanoseconds(now_ns + 60 * 60 * 1_000_000_000)
                .unwrap()
                .into(),
        })
    }
}

impl Ibc {
    #[call]
    pub fn deliver_tx(&mut self, msg: IbcTx) -> Result<()> {
        #[cfg(feature = "abci")]
        return self.exec_deliver_tx(msg);

        #[cfg(not(feature = "abci"))]
        panic!()
    }

    #[cfg(feature = "abci")]
    fn exec_deliver_tx(&mut self, msg: IbcTx) -> Result<()> {
        let mut outputs = vec![];
        for message in msg.0 {
            let output = match message {
                IbcMessage::Ics26(message) => dispatch(self, *message.clone())
                    .map_err(|e| crate::Error::Ibc(e.to_string()))?,
                IbcMessage::Ics20(message) => {
                    let signer = self.signer()?;
                    let sender_addr: Address = message
                        .sender
                        .clone()
                        .try_into()
                        .map_err(|_| crate::Error::Ibc("Invalid message sender".into()))?;
                    if sender_addr != signer {
                        return Err(Error::Ibc("Unauthorized account action".into()));
                    }

                    let mut transfer_output = HandlerOutputBuilder::new();
                    send_transfer(&mut self.transfers, &mut transfer_output, message)
                        .map_err(|e| crate::Error::Ibc(e.to_string()))?;
                    transfer_output.with_result(())
                }
            };
            outputs.push(output);
        }

        self.build_events(outputs)
    }

    pub fn bank_mut(&mut self) -> &mut transfer::Bank {
        &mut self.transfers.bank
    }

    pub fn bank(&self) -> &transfer::Bank {
        &self.transfers.bank
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Signer("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Unauthorized account action".into()))
    }

    #[cfg(feature = "abci")]
    fn build_events(&mut self, outputs: Vec<HandlerOutput<(), IbcEvent>>) -> Result<()> {
        let ctx = match self.context::<Events>() {
            Some(ctx) => ctx,
            None => return Ok(()),
        };

        for output in outputs {
            for event in output.events.into_iter() {
                let abci_event = tendermint::abci::Event::try_from(event)
                    .map_err(|e| crate::Error::Ibc(format!("{}", e)))?;
                let tm_proto_event = Event {
                    r#type: abci_event.type_str,
                    attributes: abci_event
                        .attributes
                        .into_iter()
                        .map(|attr| EventAttribute {
                            key: attr.key.as_ref().into(),
                            value: attr.value.as_ref().into(),
                            index: true,
                        })
                        .collect(),
                };

                ctx.add(tm_proto_event);
            }
        }

        Ok(())
    }

    #[call]
    pub fn transfer(&mut self, opts: TransferOpts) -> Result<()> {
        let signer = self.signer()?;
        let amt: u64 = opts.amount.into();
        let msg_transfer = MsgTransfer {
            token: Coin {
                amount: amt.to_string(),
                denom: String::from_utf8(opts.denom.0.to_vec())
                    .map_err(|_| crate::Error::Ibc("Invalid denom".into()))?,
            },
            receiver: opts.receiver.into_inner(),
            sender: signer
                .to_string()
                .parse()
                .map_err(|_| crate::Error::Ibc("Invalid sender address".into()))?,
            source_channel: opts.channel_id.into_inner(),
            source_port: opts.port_id.into_inner(),
            timeout_height: opts.timeout_height.into_inner(),
            timeout_timestamp: opts.timeout_timestamp.into_inner(),
        };

        let ibc_tx = IbcTx(vec![IbcMessage::Ics20(msg_transfer)]);

        self.deliver_tx(ibc_tx)
    }

    #[query]
    pub fn all_packet_commitments(
        &self,
        ids: Adapter<(PortId, ChannelId)>,
    ) -> Result<Vec<PacketState>> {
        let commitments = self.channels.packet_commitments(ids.clone())?;
        let transfer_commitments = self.transfers.packet_commitments(ids)?;

        let commitments = [commitments, transfer_commitments].concat();

        Ok(commitments)
    }

    #[cfg(feature = "abci")]
    pub fn raw_transfer(&mut self, message: MsgTransfer) -> Result<()> {
        let mut transfer_output = HandlerOutputBuilder::new();
        send_transfer(&mut self.transfers, &mut transfer_output, message)
            .map_err(|e| crate::Error::Ibc(e.to_string()))?;
        self.build_events(vec![transfer_output.with_result(())])
    }
}

#[cfg(feature = "abci")]
impl BeginBlock for Ibc {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.height = ctx.height;
        self.transfers.height = ctx.height;
        self.channels.height = ctx.height;
        self.connections.height = ctx.height;
        self.clients.begin_block(ctx)
    }
}

use crate::store::Read;
#[cfg(feature = "abci")]
impl AbciQuery for Ibc {
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        use ibc::core::ics02_client::context::ClientReader;
        use ibc::core::ics24_host::path::ClientStatePath;
        use ibc::core::ics24_host::Path;
        let path =
            from_utf8(&req.data).map_err(|_| crate::Error::Ibc("Invalid path encoding".into()))?;
        let path: Path = path
            .parse()
            .map_err(|_| crate::Error::Ibc(format!("Invalid path: {}", path)))?;

        if path.is_provable() {
            let maybe_value_bytes = self.lunchbox.0.get(path.clone().into_bytes().as_slice())?;
            let value_bytes = maybe_value_bytes.unwrap_or_default();

            let key = path.clone().into_bytes();

            use prost::Message;

            let mut outer_proof_bytes = vec![];
            let inner_root_hash = self
                .lunchbox
                .0
                .backing_store()
                .borrow()
                .use_merkstore(|store| store.merk().root_hash());

            let outer_proof = ics23::CommitmentProof {
                proof: Some(ics23::commitment_proof::Proof::Exist(
                    ics23::ExistenceProof {
                        key: b"ibc".to_vec(),
                        value: inner_root_hash.to_vec(),
                        leaf: Some(LeafOp {
                            hash: 6,
                            length: 0,
                            prehash_key: 0,
                            prehash_value: 0,
                            prefix: vec![],
                        }),
                        path: vec![],
                    },
                )),
            };
            outer_proof
                .encode(&mut outer_proof_bytes)
                .map_err(|_| Error::Ibc("Failed to create outer proof".into()))?;

            let mut proof_bytes = vec![];
            let proof = self
                .lunchbox
                .0
                .backing_store()
                .borrow()
                .use_merkstore(|store| store.create_ics23_proof(key.as_slice()))?;

            proof
                .encode(&mut proof_bytes)
                .map_err(|_| Error::Ibc("Failed to create proof".into()))?;

            return Ok(ResponseQuery {
                code: 0,
                key: req.data.clone(),
                value: value_bytes,
                proof_ops: Some(ProofOps {
                    ops: vec![
                        ProofOp {
                            r#type: "".to_string(),
                            key: path.into_bytes(),
                            data: proof_bytes,
                        },
                        ProofOp {
                            r#type: "".to_string(),
                            key: b"ibc".to_vec(),
                            data: outer_proof_bytes,
                        },
                    ],
                }),
                height: self.height as i64,
                ..Default::default()
            });
        }

        let value_bytes = match path {
            Path::ClientState(ClientStatePath(client_id)) => {
                let client_state = self
                    .client_state(&client_id)
                    .map_err(|_| crate::Error::Ibc("Failed to read client state".into()))?;
                client_state
                    .encode_vec()
                    .map_err(|_| crate::Error::Ibc("Failed to encode client state".into()))?
            }

            Path::ClientConsensusState(data) => {
                let client_consensus_state = self
                    .consensus_state(
                        &data.client_id,
                        Height::new(data.epoch, data.height)
                            .map_err(|_| crate::Error::Ibc("Invalid height".into()))?,
                    )
                    .map_err(|_| {
                        crate::Error::Ibc("Failed to read client consensus state".into())
                    })?;
                client_consensus_state.encode_vec().map_err(|_| {
                    crate::Error::Ibc("Failed to encode client consensus state".into())
                })?
            }

            _ => {
                return Err(crate::Error::Ibc(format!(
                    "Unsupported path query: {}",
                    path
                )))
            }
        };

        Ok(ResponseQuery {
            code: 0,
            key: req.data.clone(),
            codespace: "".to_string(),
            log: "".to_string(),
            value: value_bytes,
            proof_ops: Some(ProofOps::default()),
            ..Default::default()
        })
    }
}
