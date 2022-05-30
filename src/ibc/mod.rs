use std::convert::TryFrom;
use std::str::from_utf8;

use crate::abci::{AbciQuery, BeginBlock};
use crate::call::Call;
use crate::client::Client;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::BeginBlockCtx;
use crate::plugins::Events;
use crate::query::Query as QueryTrait;
use crate::state::State;
use crate::Result;
use client::ClientStore;
use encoding::*;
use ibc::applications::ics20_fungible_token_transfer::context::Ics20Context;
use ibc::core::ics26_routing::handler::dispatch;
use tendermint_proto::abci::{EventAttribute, RequestQuery, ResponseQuery};

mod channel;
mod client;
mod connection;
mod encoding;
pub mod path;
use path::{Identifier, Path};
mod port;
mod routing;

mod grpc;
pub use grpc::start_grpc;
pub use routing::Ics26Message;
use tendermint_proto::abci::Event;

#[derive(State, Call, Client)]
pub struct Ibc {
    client: ClientStore,
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
}

#[derive(Encode)]
pub enum Query {
    ClientStates,
}

impl Decode for Query {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        println!("decoding IBC query");
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        let path = Path::try_from(bytes.as_slice()).map_err(|_| ed::Error::UnexpectedByte(0))?;
        dbg!(&path);

        todo!()
    }
}

impl QueryTrait for Ibc {
    type Query = Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        todo!()
    }
}

impl BeginBlock for Ibc {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.client.begin_block(ctx)
    }
}

impl AbciQuery for Ibc {
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        println!("reached ibc module's abci query handler");
        dbg!(&req);
        use ibc::core::ics02_client::context::ClientReader;
        use ibc::core::ics24_host::path::ClientStatePath;
        use ibc::core::ics24_host::Path;
        let path =
            from_utf8(&req.data).map_err(|_| crate::Error::Ibc("Invalid path encoding".into()))?;
        // let path =
        dbg!(&path);
        let path: Path = path
            .parse()
            .map_err(|_| crate::Error::Ibc(format!("Invalid path: {}", path)))?;

        dbg!(&path);
        if let Path::ClientState(ClientStatePath(client_id)) = path {
            let client_state = self.client_state(&client_id).unwrap();
            dbg!(client_state);
        }
        todo!()
    }
}

impl Ics20Context for Ibc {}
