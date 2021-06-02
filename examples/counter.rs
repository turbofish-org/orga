// use failure::bail;
// use orga::abci::{ABCIStateMachine, Application, MemStore};
// use orga::encoding::Decode;
// use orga::macros::state;
// use orga::state::Value;
// use orga::store::Substore;
// use orga::Result;
// use tendermint_proto::abci::*;

// struct CounterApp;

// #[state]
// struct CounterState<S: Store> {
//     count: Value<u32>,
// }

// impl CounterApp {
//     fn run<S: Store>(&self, store: S, tx: &[u8]) -> Result<()> {
//         if tx.len() != 4 {
//             bail!("Transaction must be exactly 4 bytes");
//         }
//         let n = u32::decode(tx)?;

//         let mut state: CounterState<_> = store.wrap()?;
//         let count = state.count.get_or_default()?;

//         if n != count {
//             bail!("Incorrect count");
//         }

//         state.count.set(count + 1)?;

//         Ok(())
//     }
// }

// impl Application for CounterApp {
//     fn deliver_tx<S: Store>(&self, store: S, req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
//         let bytes = req.tx;
//         self.run(store, &bytes)?;
//         Ok(Default::default())
//     }

//     fn check_tx<S: Store>(&self, store: S, req: RequestCheckTx) -> Result<ResponseCheckTx> {
//         let bytes = req.tx;
//         self.run(store, &bytes)?;
//         Ok(Default::default())
//     }
// }

// pub fn main() {
//     let store = MemStore::new();
//     ABCIStateMachine::new(CounterApp, store)
//         .listen("127.0.0.1:26658")
//         .unwrap();
// }
