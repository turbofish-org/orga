use orga::abci::{Application, ABCIStateMachine};
use orga::encoding::Decode;
use orga::merkstore::MerkStore;
use orga::state::Value;
use orga::store::Store;
use orga::Result;
use orga::macros::state;
use abci2::messages::abci::*;
use failure::bail;

struct CounterApp;

#[state]
struct CounterState<S: Store> {
    count: Value<u32>
}

impl CounterApp {
    fn run<S: Store>(&self, store: S, tx: &[u8]) -> Result<()> {
        if tx.len() != 4 {
            bail!("Transaction must be exactly 4 bytes");
        }
        let n = u32::decode(tx)?;

        let mut state: CounterState<_> = store.wrap()?;
        let count = state.count.get_or_default()?;

        if n != count {
            bail!("Incorrect count");
        }

        state.count.set(count + 1)?;

        Ok(())
    }
}

impl Application for CounterApp {
    fn deliver_tx<S: Store>(&self, store: S, req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        let bytes = req.get_tx();
        self.run(store, bytes)?;
        Ok(Default::default())
    }

    fn check_tx<S: Store>(&self, store: S, req: RequestCheckTx) -> Result<ResponseCheckTx> {
        let bytes = req.get_tx();
        self.run(store, bytes)?;
        Ok(Default::default())
    }
}

pub fn main() {
    let mut m = merk::test_utils::TempMerk::new().unwrap();
    let store = MerkStore::new(&mut m);
    ABCIStateMachine::new(CounterApp, store)
        .listen("localhost:26658")
        .unwrap();
}
