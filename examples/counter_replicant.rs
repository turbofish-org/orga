use ed::Encode;
use orga::abci::ABCIStateMachine;
use orga::abci::ABCIStore;
use orga::abci::Application;
use orga::encoding::Decode;
use orga::merk::MerkStore;
use orga::state::State;
use orga::store::Shared;
use orga::store::{BufStore, Read, Store, Write};
use std::fs;
use std::path::Path;
struct App;

#[derive(State)]
struct CounterState {
    count: u32,
}

impl Application for App {
    fn init_chain(
        &self,
        _store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        _req: tendermint_proto::abci::RequestInitChain,
    ) -> orga::Result<tendermint_proto::abci::ResponseInitChain> {
        Ok(Default::default())
    }

    fn deliver_tx(
        &self,
        store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        _req: tendermint_proto::abci::RequestDeliverTx,
    ) -> orga::Result<tendermint_proto::abci::ResponseDeliverTx> {
        let mut store = Store::new(store);

        let store_val = match store.get(&[]) {
            Ok(Some(inner)) => inner,
            Ok(None) => {
                let default: u32 = Default::default();
                Encode::encode(&default).unwrap()
            }
            Err(_) => panic!("Store get failed"),
        };

        let data = Decode::decode(store_val.as_slice()).unwrap();

        let mut state: CounterState = CounterState::create(store.clone(), data).unwrap();
        state.count += 1;
        let flushed = state.flush().unwrap();
        store.put(vec![], flushed.encode().unwrap()).unwrap();
        Ok(Default::default())
    }
    fn begin_block(
        &self,
        store: Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>,
        _req: tendermint_proto::abci::RequestBeginBlock,
    ) -> orga::Result<tendermint_proto::abci::ResponseBeginBlock> {
        let store = Store::new(store);
        let store_val = match store.get(&[]) {
            Ok(Some(inner)) => inner,
            Ok(None) => {
                let default: u32 = Default::default();
                Encode::encode(&default).unwrap()
            }
            Err(_) => panic!("Store get failed"),
        };
        let data = Decode::decode(store_val.as_slice()).unwrap();
        let state: CounterState = CounterState::create(store, data).unwrap();
        println!("The count is: {}", state.count);
        Ok(Default::default())
    }
}

fn main() {
    let app = App {};
    let dir = "./counter_replicant.db";
    let store = orga::merk::MerkStore::new(dir.into());
    ABCIStateMachine::new(app, store)
        .listen("127.0.0.1:26678")
        .unwrap();
}
