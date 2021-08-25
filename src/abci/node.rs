use std::marker::PhantomData;

use super::CONTEXT;
use super::{ABCIStateMachine, App, Application};
use crate::encoding::{Decode, Encode};
use crate::merk::MerkStore;
use crate::state::State;
use crate::store::{BufStore, Read, Shared, Store, Write};
use crate::Result;
pub struct Node<A: App>
where
    <A as State>::Encoding: Default,
{
    abci_sm: ABCIStateMachine<InternalApp<A>>,
    _app: PhantomData<A>,
}

impl<A: App> Node<A>
where
    <A as State>::Encoding: Default,
{
    pub fn new(home: &str) -> Self {
        let app = InternalApp::<A>::new();
        let store = MerkStore::new(home.into());

        let abci_sm = ABCIStateMachine::new(app, store);
        Node {
            _app: PhantomData,
            abci_sm,
        }
    }

    pub fn run(self) {
        self.abci_sm
            .listen("127.0.0.1:26658")
            .expect("Failed to start ABCI server");
    }
}

impl<A: App> Application for InternalApp<A>
where
    <A as State>::Encoding: Default,
{
    fn init_chain(
        &self,
        store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        _req: tendermint_proto::abci::RequestInitChain,
    ) -> Result<tendermint_proto::abci::ResponseInitChain> {
        let mut store = Store::new(store.into());

        let state_bytes = match store.get(&[])? {
            Some(inner) => inner,
            None => {
                let default_encoding: A::Encoding = Default::default();
                let encoded_bytes = Encode::encode(&default_encoding).unwrap();
                store.put(vec![], encoded_bytes.clone())?;
                encoded_bytes
            }
        };
        let data: <A as State>::Encoding = Decode::decode(state_bytes.as_slice())?;
        // TODO: double-check that default encoding is the same as if we made a
        // state, flushed, encoded, and wrote that instead.
        let mut state = <A as State>::create(store.clone(), data)?;
        A::init_chain(&mut state)?;
        let flushed = state.flush()?;
        store.put(vec![], flushed.encode()?)?;

        Ok(Default::default())
    }

    fn begin_block(
        &self,
        store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        req: tendermint_proto::abci::RequestBeginBlock,
    ) -> Result<tendermint_proto::abci::ResponseBeginBlock> {
        // Set context
        {
            let mut ctx = CONTEXT.lock().unwrap();
            if let Some(header) = req.header.as_ref() {
                ctx.height = header.height as u64;
            }
            ctx.header = req.header;
        };
        // Step state machine
        let mut store = Store::new(store.into());
        let state_bytes = store.get(&[])?.unwrap();
        let data: <A as State>::Encoding = Decode::decode(state_bytes.as_slice())?;
        let mut state = <A as State>::create(store.clone(), data)?;
        A::begin_block(&mut state)?;
        let flushed = state.flush()?;
        store.put(vec![], flushed.encode()?)?;

        Ok(Default::default())
    }

    fn end_block(
        &self,
        store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        _req: tendermint_proto::abci::RequestEndBlock,
    ) -> Result<tendermint_proto::abci::ResponseEndBlock> {
        let mut store = Store::new(store.into());
        let state_bytes = store.get(&[])?.unwrap();
        let data: <A as State>::Encoding = Decode::decode(state_bytes.as_slice())?;
        let mut state = <A as State>::create(store.clone(), data)?;
        A::end_block(&mut state)?;
        let flushed = state.flush()?;
        store.put(vec![], flushed.encode()?)?;
        // Send back validator updates
        let mut res: tendermint_proto::abci::ResponseEndBlock = Default::default();
        {
            let mut ctx = CONTEXT.lock().unwrap();
            ctx.validator_updates.drain().for_each(|(_key, update)| {
                res.validator_updates.push(update);
            });
        }

        Ok(res)
    }

    fn deliver_tx(
        &self,
        store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        req: tendermint_proto::abci::RequestDeliverTx,
    ) -> Result<tendermint_proto::abci::ResponseDeliverTx> {
        let mut store = Store::new(store.into());
        let state_bytes = store.get(&[])?.unwrap();
        let data: <A as State>::Encoding = Decode::decode(state_bytes.as_slice())?;
        let mut state = <A as State>::create(store.clone(), data)?;
        // TODO: handle call message here
        println!("Warning: got a TX, did nothing with it");
        let flushed = state.flush()?;
        store.put(vec![], flushed.encode()?)?;

        Ok(Default::default())
    }
}

struct InternalApp<A: App>
where
    <A as State>::Encoding: Default,
{
    _app: PhantomData<A>,
}

impl<A: App> InternalApp<A>
where
    <A as State>::Encoding: Default,
{
    pub fn new() -> Self {
        Self { _app: PhantomData }
    }
}
