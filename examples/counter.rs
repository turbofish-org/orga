use ed::Encode;
use orga::abci::Application;
use orga::abci::{ABCIStateMachine, ABCIStore};
use orga::collections::Map;
use orga::merk::MerkStore;
use orga::state::State;
use orga::state_machine::step_atomic;
use orga::store::{BufStore, DefaultBackingStore, NullStore, Read, Store, Write};

struct App;

#[derive(State)]
struct CounterState {
    count: u32,
    count_map: Map<u32, u32>,
}

fn foo(state: &mut CounterState) {
    state.count = state.count + 1;
}

fn main() {
    use orga::encoding::{Decode, Encode};
    use orga::store::{Read, Shared, Write};
    let mut store_a = Shared::new(MerkStore::new("./counter-test.db".into()));
    let mut store_b = Shared::new(BufStore::wrap_with_map(store_a.clone(), Default::default()));
    let mut store_c = Shared::new(BufStore::wrap_with_map(store_b.clone(), Default::default()));
    let mut store = Store::new(store_c.clone());

    let data = Decode::decode(store.get(&[]).unwrap().unwrap().as_slice()).unwrap();
    println!("data: {:?}", data);
    let mut state = CounterState::create(store.clone(), data).unwrap();
    foo(&mut state);
    let flushed = state.flush().unwrap();
    store_c.put(vec![], flushed.encode().unwrap()).unwrap();
    drop(store);

    store_c.into_inner().flush().unwrap();
    store_b.into_inner().flush().unwrap();
    let mut store_a = store_a.into_inner();
    store_a.commit(store_a.height().unwrap() + 1).unwrap();
}

// impl Application<DefaultBackingStore> for App {
//     fn init_chain(
//         &self,
//         store: DefaultBackingStore,
//         _req: tendermint_proto::abci::RequestInitChain,
//     ) -> orga::Result<tendermint_proto::abci::ResponseInitChain> {
//         Ok(Default::default())
//     }

// fn deliver_tx(
//     &self,
//     store: DefaultBackingStore,
//     req: tendermint_proto::abci::RequestDeliverTx,
// ) -> orga::Result<tendermint_proto::abci::ResponseDeliverTx> {
//     let state: CounterState = CounterState::create(store, Default::default()).unwrap();
//     state.count += 1;
//     state.flush();
//     Ok(Default::default())
// }
//     fn begin_block(
//         &self,
//         store: MerkStore,
//         _req: tendermint_proto::abci::RequestBeginBlock,
//     ) -> orga::Result<tendermint_proto::abci::ResponseBeginBlock> {
//         let state: CounterState = CounterState::create(store, Default::default()).unwrap();
//         println!("The count is: {}", state.count);
//         Ok(Default::default())
//     }*/
// }

// fn main() {
//     let app = App {};
//     let store = orga::merk::MerkStore::new("./counter.db".into());
//     ABCIStateMachine::new(app, store)
//         .listen("localhost:26658")
//         .unwrap();
// }
