use orga::abci::Application;
use orga::abci::{ABCIStateMachine, ABCIStore};
use orga::merk::MerkStore;
use orga::state::State;
use orga::state_machine::step_atomic;
use orga::store::{BufStore, DefaultBackingStore, NullStore, Read, Store, Write};

struct App;

//#[derive(State)]
struct CounterState<S = &'static mut BufStore<&'static mut BufStore<&'static mut MerkStore>>> {
    count: u32,
    count_map: orga::collections::Map<u32, u32, S>,
}

impl<S> State<S> for CounterState<S> {
    type Encoding = (
        <u32 as ::orga::state::State<S>>::Encoding,
        <::orga::collections::Map<u32, u32, S> as ::orga::state::State<S>>::Encoding,
    );

    fn create(store: Store<S>, data: Self::Encoding) -> orga::Result<Self>
    where
        S: Read,
    {
        Ok(CounterState {
            count: ::orga::state::State::<S>::create(store.sub(&[0]), data.0)?,
            count_map: orga::state::State::<S>::create(store.sub(&[1]), data.1)?,
        })
    }

    fn flush(self) -> ::orga::Result<Self::Encoding>
    where
        S: Write,
    {
        Ok((
            State::<S>::flush(self.count)?,
            State::<S>::flush(self.count_map)?,
        ))
    }
}

impl<S> From<CounterState<S>>
    for (
        <u32 as ::orga::state::State<S>>::Encoding,
        <::orga::collections::Map<u32, u32, S> as ::orga::state::State<S>>::Encoding,
    )
{
    fn from(state: CounterState<S>) -> Self {
        (state.count.into(), state.count_map.into())
    }
}

fn foo<S: Read + Write>(store: Store<S>) -> u32 {
    //&mut BufStore<&mut BufStore<&mut S>>) {
    let mut state = CounterState::create(store, Default::default()).unwrap();
    let mut count = state
        .count_map
        .entry(42)
        .unwrap()
        .or_insert_default()
        .unwrap();
    let c = *count;
    println!("state.count before increment: {}", c);
    state.count_map.insert(42, c + 1).unwrap();
    state.flush();
    c
}

// fn pretty_app(state: CounterState, tx: u32) {

// }

fn main() {
    //    let store: Store<DefaultBackingStore> = Store::new(MerkStore::new(std::path::PathBuf::new()));
    let mut store_a = MerkStore::new("./counter-test.db".into());
    let mut store_b = BufStore::wrap_with_map(&mut store_a, Default::default());
    let mut store_c = BufStore::wrap_with_map(&mut store_b, Default::default());
    let mut store = Store::new(&mut store_c);
    let height = foo(store);
    store_c.flush();
    store_b.flush();
    store_a.commit(height as u64).unwrap();
    // foo(store);
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
