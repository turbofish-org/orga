use crate::error::Result;
use crate::store::{Store, MapStore, Flush};

pub trait StateMachine<A> {
    fn step<S: Store>(&mut self, action: A, store: &mut S) -> Result<()>;

    // TODO: this needs a better name
    // TODO: is there a way to implement this elsewhere? adding provided methods to the trait doesn't scale
    fn step_flush<S>(&mut self, action: A, store: &mut S) -> Result<()>
        where S: Store
    {
        let mut flush_store = MapStore::wrap(store);

        match self.step(action, &mut flush_store) {
            Err(err) => Err(err),
            Ok(_) => flush_store.finish().flush(store)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::{Error, ErrorKind};
    use crate::store::{ErrorKind::NotFound, MapStore, Read};
    use super::*;

    struct CounterSM;

    impl StateMachine<u8> for CounterSM {
        fn step<S: Store>(&mut self, n: u8, store: &mut S) -> Result<()> {
            // set this before checking if `n` is valid, so we can test state
            // mutations on invalid txs
            self.put(b"n", n, store)?;

            // get count, compare to n, write if successful
            let count = self.get(b"count", store)?;
            if count != n {
                return Err("Invalid count".into());
            }
            self.put(b"count", count + 1, store)
        }
    }

    impl CounterSM {
        fn get<S: Store>(&mut self, key: &[u8], store: &mut S) -> Result<u8> {
            match store.get(key) {
                Err(Error(ErrorKind::Store(crate::store::Error(NotFound, _)), _)) => Ok(0),
                Ok(vec) => Ok(vec[0]),
                Err(err) => return Err(err)
            }
        }

        fn put<S: Store>(&mut self, key: &[u8], value: u8, store: &mut S) -> Result<()> {
            store.put(key.to_vec(), vec![value])
        }
    }

    #[test]
    fn step_counter_error() {
        let mut store = MapStore::new();
        // invalid `n`, should error
        assert!(CounterSM.step(100, &mut store).is_err());
        // count should not have been mutated
        assert!(store.get(b"count").is_err());
        // n should have been mutated
        assert_eq!(store.get(b"n").unwrap(), vec![100]);
    }

    #[test]
    fn step_counter_error_flusher() {
        let mut store = MapStore::new();
        // invalid `n`, should error
        assert!(CounterSM.step_flush(100, &mut store).is_err());
        // count should not have been mutated
        assert!(store.get(b"count").is_err());
        // n should not have been mutated
        assert!(store.get(b"n").is_err());
    }

    #[test]
    fn step_counter() {
        let mut store = MapStore::new();
        assert!(CounterSM.step_flush(0, &mut store).is_ok());
        assert!(CounterSM.step_flush(0, &mut store).is_err());
        assert!(CounterSM.step_flush(1, &mut store).is_ok());
        assert!(CounterSM.step_flush(1, &mut store).is_err());
        assert_eq!(store.get(b"n").unwrap(), vec![1]);
        assert_eq!(store.get(b"count").unwrap(), vec![2]);
    }
}
