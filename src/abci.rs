use std::net::ToSocketAddrs;
use abci2::messages::abci;
use crate::{StateMachine, Store, WriteCache, Result};

pub trait ABCIStateMachine: StateMachine<abci::Request, abci::Response> {
    fn listen<A: ToSocketAddrs>(&self, addr: A) -> Result<()> {
        Ok(())
    }
}

impl<S> ABCIStateMachine for S
    where S: StateMachine<abci::Request, abci::Response>
{}

#[cfg(test)]
mod tests {
    use super::*;

    fn null_abcism(_: &mut dyn Store, _: abci::Request) -> Result<abci::Response> {
        Ok(abci::Response::new())
    }

    #[test]
    fn gets_abci_impls() {
        null_abcism.listen("localhost:26658").unwrap();
    }
}
