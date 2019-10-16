mod atom;

use crate::error::Result;
use crate::store::Store;

pub use atom::Atom;

pub trait StateMachine {
    type Input;
    type Output = ();

    fn step<S>(&mut self, input: Self::Input, store: &mut S) -> Result<Self::Output>
        where S: Store;
}

pub trait Atomic: StateMachine {}

#[cfg(test)]
mod tests {
    use crate::store::{WriteCache, Read};
    use super::*;

    struct CounterSM;

    impl StateMachine for CounterSM {
        type Input = u8;
        type Output = u8;

        fn step<S: Store>(&mut self, n: u8, store: &mut S) -> Result<u8> {
            // set this before checking if `n` is valid, so we can test state
            // mutations on invalid txs
            self.put(b"n", n, store)?;

            // get count, compare to n, write if successful
            let count = self.get(b"count", store)?;
            if count != n {
                return Err("Invalid count".into());
            }
            self.put(b"count", count + 1, store)?;
            Ok(count + 1)
        }
    }

    impl CounterSM {
        fn get<S: Store>(&mut self, key: &[u8], store: &mut S) -> Result<u8> {
            match store.get(key) {
                Ok(None) => Ok(0),
                Ok(Some(vec)) => Ok(vec[0]),
                Err(err) => return Err(err)
            }
        }

        fn put<S: Store>(&mut self, key: &[u8], value: u8, store: &mut S) -> Result<()> {
            store.put(key.to_vec(), vec![value])
        }
    }

    #[test]
    fn step_counter_error() {
        let mut store = WriteCache::new();
        // invalid `n`, should error
        assert!(CounterSM.step(100, &mut store).is_err());
        // count should not have been mutated
        assert_eq!(store.get(b"count").unwrap(), None);
        // n should have been mutated
        assert_eq!(store.get(b"n").unwrap(), Some(vec![100]));
    }

    #[test]
    fn step_counter_atomic_error() {
        let mut store = WriteCache::new();
        let mut sm = Atom::new(CounterSM);
        // invalid `n`, should error
        assert!(sm.step(100, &mut store).is_err());
        // count should not have been mutated
        assert_eq!(store.get(b"count").unwrap(), None);
        // n should not have been mutated
        assert_eq!(store.get(b"n").unwrap(), None);
    }

    #[test]
    fn step_counter() {
        let mut store = WriteCache::new();
        let mut sm = Atom::new(CounterSM);
        assert_eq!(sm.step(0, &mut store).unwrap(), 1);
        assert!(sm.step(0, &mut store).is_err());
        assert_eq!(sm.step(1, &mut store).unwrap(), 2);
        assert!(sm.step(1, &mut store).is_err());
        assert_eq!(store.get(b"n").unwrap(), Some(vec![1]));
        assert_eq!(store.get(b"count").unwrap(), Some(vec![2]));
    }
}
