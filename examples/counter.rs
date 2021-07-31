use orga::abci::Application;
use orga::abci::{ABCIStateMachine, ABCIStore};
use orga::merk::MerkStore;
use orga::state::State;
use orga::state_machine::step_atomic;
use orga::store::{BufStore, DefaultBackingStore, NullStore, Read, Store, Write};

struct App;

#[derive(State)]
struct CounterState {
    count: u32,
}

fn foo<S: Read + Write>(store: S) {
    let mut store = Store::new(store);
    let mut store = Store::new(BufStore::wrap(store));
    let state: CounterState = CounterState::create(store, Default::default()).unwrap();
}

fn main() {}

// impl Application<DefaultBackingStore> for App {
//     fn init_chain(
//         &self,
//         store: DefaultBackingStore,
//         _req: tendermint_proto::abci::RequestInitChain,
//     ) -> orga::Result<tendermint_proto::abci::ResponseInitChain> {
//         Ok(Default::default())
//     }

//     fn deliver_tx(
//         &self,
//         store: DefaultBackingStore,
//         req: tendermint_proto::abci::RequestDeliverTx,
//     ) -> orga::Result<tendermint_proto::abci::ResponseDeliverTx> {
//         let state: CounterState = CounterState::create(store, Default::default()).unwrap();
//         state.count += 1;
//         state.flush();
//         Ok(Default::default())
//     }
//     /*
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
