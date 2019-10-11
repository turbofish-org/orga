use crate::error::Result;
use crate::store::Store;

pub trait StateMachine<A> {
    fn step<S: Store>(&mut self, action: A, store: S) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use crate::error::{Error, ErrorKind};
    use crate::store::{ErrorKind::NotFound, MapStore};
    use super::*;

    struct CounterSM ();

    impl StateMachine<u8> for CounterSM {
        fn step<S: Store>(&mut self, n: u8, mut store: S) -> Result<()> {
            let count = match store.get(b"count") {
                Err(Error(ErrorKind::Store(crate::store::Error(NotFound, _)), _)) => 0,
                Ok(vec) => vec[0],
                Err(err) => return Err(err)
            };

            if count != n {
                return Err("Invalid count".into());
            }

            store.put(b"count".to_vec(), vec![count + 1])?;

            Ok(())
        }
    }

    #[test]
    fn step_counter_error() {
        let mut sm = CounterSM();
        let store = MapStore::new();
        assert!(sm.step(100, store).is_err());
    }

    #[test]
    fn step_counter() {
        let mut sm = CounterSM();
        let store = MapStore::new();
        assert!(sm.step(0, store).is_ok());
    }
}
