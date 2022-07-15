use std::convert::TryFrom;
use std::str::from_utf8;

use crate::abci::{AbciQuery, BeginBlock};
use crate::call::Call;
use crate::client::Client;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::BeginBlockCtx;
use crate::plugins::Events;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use client::ClientStore;
use encoding::*;
use ibc::applications::transfer::context::Ics20Context;
use ibc::core::ics02_client::height::Height;
use ibc::core::ics26_routing::handler::dispatch;
use ics23::LeafOp;
use sha2::Sha512_256;
use tendermint_proto::abci::{EventAttribute, RequestQuery, ResponseQuery};
use tendermint_proto::Protobuf;

mod channel;
mod client;
mod connection;
mod encoding;
pub mod path;
use path::{Identifier, Path};
mod port;
mod routing;

mod grpc;
use crate::store::Store;
pub use grpc::start_grpc;
pub use routing::Ics26Message;
use tendermint_proto::abci::Event;
use tendermint_proto::crypto::{ProofOp, ProofOps};

use self::connection::ConnectionStore;

#[derive(State, Call, Client, Query)]
pub struct Ibc {
    pub client: ClientStore,
    pub connections: ConnectionStore,
    height: u64,
    pub(super) lunchbox: Lunchbox,
}

pub struct Lunchbox(pub(super) Store);

impl State for Lunchbox {
    type Encoding = ();
    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
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
    pub fn deliver_message(&mut self, msg: Ics26Message) -> Result<()> {
        println!("made deliver_message call: {:#?}", msg);
        let output = dispatch(self, msg.0.first().unwrap().clone())
            .map_err(|e| crate::Error::Ibc(e.to_string()))?;

        let ctx = self
            .context::<Events>()
            .ok_or_else(|| crate::Error::Ibc("No Events context available".into()))?;

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

        Ok(())
    }

    // #[query]
    // pub fn client_states(&self) -> Result<()> {
    //     println!("queried client states with query method");
    //     let client_states = self.client.query_client_states()?;
    //     Ok(())
    // }
}

// #[derive(Encode)]
// pub enum Query {
//     ClientStates,
// }

// impl Decode for Query {
//     fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
//         println!("decoding IBC query");
//         let mut bytes = vec![];
//         reader.read_to_end(&mut bytes)?;
//         let path = Path::try_from(bytes.as_slice()).map_err(|_| ed::Error::UnexpectedByte(0))?;
//         dbg!(&path);

//         todo!()
//     }
// }

// impl QueryTrait for Ibc {
//     type Query = Query;

//     fn query(&self, query: Self::Query) -> Result<()> {
//         todo!()
//     }
// }

impl BeginBlock for Ibc {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.height = ctx.height;
        self.client.begin_block(ctx)
    }
}

use crate::store::Read;
impl AbciQuery for Ibc {
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        println!("reached ibc module's abci query handler");
        use ibc::core::ics02_client::context::ClientReader;
        use ibc::core::ics24_host::path::ClientStatePath;
        use ibc::core::ics24_host::Path;
        let path =
            from_utf8(&req.data).map_err(|_| crate::Error::Ibc("Invalid path encoding".into()))?;
        let path: Path = path
            .parse()
            .map_err(|_| crate::Error::Ibc(format!("Invalid path: {}", path)))?;

        println!(
            "formatted path: {}, is provable: {}, wants proof: {}",
            path,
            path.is_provable(),
            req.prove,
        );
        if path.is_provable() {
            let value_bytes = self
                .lunchbox
                .0
                .get(path.clone().into_bytes().as_slice())?
                .unwrap_or_default();

            dbg!(&req.data);
            let key = path.clone().into_bytes();

            use prost::Message;

            let mut outer_proof_bytes = vec![];
            let inner_root_hash = self
                .lunchbox
                .0
                .backing_store()
                .borrow()
                .use_merkstore(|store| store.merk().root_hash());

            ics23::CommitmentProof {
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
            }
            .encode(&mut outer_proof_bytes)
            .map_err(|_| Error::Ibc("Failed to create outer proof".into()))?;

            let mut proof_bytes = vec![];
            self.lunchbox
                .0
                .backing_store()
                .borrow()
                .use_merkstore(|store| dbg!(store.create_ics23_proof(key.as_slice())))?
                .encode(&mut proof_bytes)
                .map_err(|_| Error::Ibc("Failed to create proof".into()))?;

            use crate::abci::ABCIStore;
            let outer_app_hash = self
                .lunchbox
                .0
                .backing_store()
                .borrow()
                .use_merkstore(|store| store.root_hash())?;

            dbg!(&value_bytes.len());
            dbg!(&proof_bytes.len());

            return Ok(ResponseQuery {
                code: 0,
                key: req.data.clone(),
                codespace: "".to_string(),
                log: "".to_string(),
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
                ..Default::default()
            });
        }

        let value_bytes = match path {
            Path::ClientState(ClientStatePath(client_id)) => {
                let client_state = self.client_state(&client_id).unwrap();
                dbg!(&client_state);
                client_state.encode_vec().unwrap()
            }

            Path::ClientConsensusState(data) => {
                let client_consensus_state = self
                    .consensus_state(
                        &data.client_id,
                        Height::new(data.epoch, data.height).unwrap(),
                    )
                    .unwrap();
                dbg!(&client_consensus_state);
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

// impl Ics20Context for Ibc {}
