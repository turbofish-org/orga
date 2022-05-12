use crate::state::State;
use ibc::applications::ics20_fungible_token_transfer::context::Ics20Context;
use ibc::core::ics26_routing::context::Ics26Context;
mod channel;
mod client;
mod encoding;
use crate::call::Call;
use crate::client::Client;
use crate::query::Query;
use client::ClientStore;
use encoding::*;
mod connection;
mod port;
mod routing;
use crate::Result;
mod grpc;
pub use grpc::start_grpc;

#[derive(State, Call, Query, Client)]
pub struct Ibc {
    client: ClientStore,
}

impl Ibc {
    #[call]
    pub fn deliver_message(&mut self) -> Result<()> {
        println!("made deliver_message call!");
        Ok(())
    }
}

impl Clone for Ibc {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl Ics20Context for Ibc {}
