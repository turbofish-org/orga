use orga::abci::Application;
use orga::abci::{ABCIStateMachine, ABCIStore};
use orga::merk::MerkStore;
use orga::state::State;
use orga::state_machine::step_atomic;
use orga::store::{BufStore, DefaultBackingStore, NullStore, Read, Store, Write};

struct App;

//#[derive(State)]
struct CounterState<S> {
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

// impl From<CounterState> for <CounterState as ::orga::state::State>::Encoding {
// fn from(state: CounterState) -> Self {
//     (state.count.into(), state.count_map.into())
// }
// }

// impl<S, I: Intermediate> From<CounterState<S>> for Intermediate {
//     fn from(state: CounterState<S>) -> Self {
//         (state.count.into(), state.count_map.into())
//     }
// }
//
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

fn foo<S: Read>(store: &mut BufStore<&mut BufStore<&mut S>>) {
    let store = Store::new(store);
    let _state = CounterState::create(store, Default::default()).unwrap();
}

fn main() {
    let store: Store<DefaultBackingStore> = Store::new(MerkStore::new(std::path::PathBuf::new()));
    // let mut_store: &mut BufStore<&mut BufStore<&mut S>> = BufStore::new(BufStore::new());

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
