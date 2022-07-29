use std::convert::TryFrom;
use std::str::from_utf8;

use crate::abci::{AbciQuery, BeginBlock, InitChain};
use crate::call::Call;
use crate::client::Client;
use crate::context::{Context, GetContext};
use crate::encoding::{Decode, Encode};
use crate::merk::MerkStore;
use crate::plugins::Events;
use crate::plugins::{BeginBlockCtx, InitChainCtx};
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use client::ClientStore;
use encoding::*;
use ibc::applications::transfer::context::Ics20Context;
use ibc::applications::transfer::relay::send_transfer::send_transfer;
use ibc::core::ics02_client::height::Height;
use ibc::core::ics26_routing::context::Module;
use ibc::core::ics26_routing::handler::dispatch;
use ibc::handler::HandlerOutputBuilder;
use ics23::commitment_proof::Proof;
use ics23::{CommitmentProof, InnerSpec, LeafOp, ProofSpec};
use sha2::Sha512_256;
use tendermint_proto::abci::{EventAttribute, RequestQuery, ResponseQuery};
use tendermint_proto::Protobuf;

mod channel;
mod client;
mod connection;
mod encoding;
mod grpc;
mod port;
mod routing;
mod transfer;

use crate::store::Store;
pub use grpc::start_grpc;
use tendermint_proto::abci::Event;
use tendermint_proto::crypto::{ProofOp, ProofOps};

use self::channel::ChannelStore;
use self::connection::ConnectionStore;
use self::port::PortStore;
pub use self::routing::{IbcMessage, IbcTx};
use self::transfer::TransferModule;

#[derive(State, Call, Client, Query)]
pub struct Ibc {
    pub clients: ClientStore,
    pub connections: ConnectionStore,
    pub channels: ChannelStore,
    ports: PortStore,
    height: u64,
    pub transfers: TransferModule,
    pub(super) lunchbox: Lunchbox,
}

pub struct Lunchbox(pub(super) Store);

impl State for Lunchbox {
    type Encoding = ();
    fn create(store: Store, _data: Self::Encoding) -> Result<Self> {
        Ok(unsafe { Self(store.with_prefix(vec![])) })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(())
    }
}

impl From<Lunchbox> for () {
    fn from(_: Lunchbox) -> Self {}
}

impl Ibc {
    #[call]
    pub fn deliver_message(&mut self, msg: IbcTx) -> Result<()> {
        let mut outputs = vec![];
        for message in msg.0 {
            let output = match message {
                IbcMessage::Ics26(message) => {
                    // println!("Ics26 message: {:?}", message);
                    dispatch(self, message.clone())
                        .map_err(|e| dbg!(crate::Error::Ibc(e.to_string())))?
                }
                IbcMessage::Ics20(message) => {
                    // println!("Transfer message: {:?}", message);
                    let mut transfer_output = HandlerOutputBuilder::new();
                    send_transfer(&mut self.transfers, &mut transfer_output, message)
                        .map_err(|e| dbg!(crate::Error::Ibc(e.to_string())))?;
                    transfer_output.with_result(())
                }

                _ => return Err(crate::Error::Ibc("Unsupported IBC message".to_string())),
            };

            outputs.push(output);
        }

        let ctx = self
            .context::<Events>()
            .ok_or_else(|| dbg!(crate::Error::Ibc("No events context available".into())))?;

        for output in outputs {
            for event in output.events.into_iter() {
                let abci_event = tendermint::abci::Event::try_from(event)
                    .map_err(|e| crate::Error::Ibc(format!("{}", dbg!(e))))?;
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
}

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

        println!("path: {}, prove: {}", path, req.prove,);
        if path.is_provable() {
            let maybe_value_bytes = self.lunchbox.0.get(path.clone().into_bytes().as_slice())?;
            let value_bytes = maybe_value_bytes.unwrap_or_default();

            // dbg!(&req.data);
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

            // dbg!(ics23::verify_membership(
            //     &proof,
            //     &MerkStore::ics23_spec(),
            //     &inner_root_hash.to_vec(),
            //     key.as_slice(),
            //     value_bytes.as_slice()
            // ));

            proof
                .encode(&mut proof_bytes)
                .map_err(|_| Error::Ibc("Failed to create proof".into()))?;

            use crate::abci::ABCIStore;
            // let outer_app_hash = self
            //     .lunchbox
            //     .0
            //     .backing_store()
            //     .borrow()
            //     .use_merkstore(|store| store.root_hash())?;

            // dbg!(&value_bytes.len());
            // dbg!(&proof_bytes.len());

            // dbg!(ics23::verify_membership(
            //     &outer_proof,
            //     &ProofSpec {
            //         inner_spec: Some(InnerSpec {
            //             child_order: vec![0],
            //             child_size: 32,
            //             empty_child: vec![],
            //             min_prefix_length: 0,
            //             max_prefix_length: 0,
            //             hash: 6,
            //         }),
            //         leaf_spec: Some(LeafOp {
            //             hash: 6,
            //             length: 0,
            //             prefix: vec![],
            //             prehash_key: 0,
            //             prehash_value: 0,
            //         }),
            //         max_depth: 0,
            //         min_depth: 0,
            //     },
            //     &outer_app_hash,
            //     b"ibc",
            //     &inner_root_hash
            // ));

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
                let client_state = self.client_state(&client_id).unwrap();
                // dbg!(&client_state);
                client_state.encode_vec().unwrap()
            }

            Path::ClientConsensusState(data) => {
                let client_consensus_state = self
                    .consensus_state(
                        &data.client_id,
                        Height::new(data.epoch, data.height).unwrap(),
                    )
                    .unwrap();
                // dbg!(&client_consensus_state);
                client_consensus_state.encode_vec().unwrap()
            }

            _ => todo!(),
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

use ibc::core::ics26_routing::context::Router;
impl InitChain for Ibc {
    fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
        Ok(())
    }
}
