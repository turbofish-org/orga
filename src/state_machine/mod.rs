mod atom;
mod router;

use crate::error::Result;
use crate::store::Store;

pub use atom::step_atomic;

pub trait StateMachine<S: Store, I, O>: Fn(&mut S, I) -> Result<O> {}
impl<F, S, I, O> StateMachine<S, I, O> for F
    where
        F: Fn(&mut S, I) -> Result<O>,
        S: Store
{}

pub trait Atomic<S: Store, I, O>: StateMachine<S, I, O> {}

#[cfg(test)]
mod tests {
    use crate::store::{WriteCache, Read, Write};
    use super::*;

    fn get_u8(key: &[u8], store: &impl Read) -> Result<u8> {
        match store.get(key) {
            Ok(None) => Ok(0),
            Ok(Some(vec)) => Ok(vec[0]),
            Err(err) => Err(err)
        }
    }

    fn put_u8(key: &[u8], value: u8, store: &mut impl Write) -> Result<()> {
        store.put(key.to_vec(), vec![value])
    }

    fn counter(store: &mut impl Store, n: u8) -> Result<u8> {
        // put to n before checking count (to test atomicity)
        put_u8(b"n", n, store)?;

        // get count, compare to n, write if successful
        let count = get_u8(b"count", store)?;
        if count != n {
            return Err("Invalid count".into());
        }
        put_u8(b"count", count + 1, store)?;
        Ok(count + 1)
    }

    #[test]
    fn step_counter_error() {
        let mut store = WriteCache::new();
        // invalid `n`, should error
        assert!(counter(&mut store, 100).is_err());
        // count should not have been mutated
        assert_eq!(store.get(b"count").unwrap(), None);
        // n should have been mutated
        assert_eq!(store.get(b"n").unwrap(), Some(vec![100]));
    }

    #[test]
    fn step_counter_atomic_error() {
        let mut store = WriteCache::new();
        // invalid `n`, should error
        assert!(step_atomic(counter, &mut store, 100).is_err());
        // count should not have been mutated
        assert_eq!(store.get(b"count").unwrap(), None);
        // n should not have been mutated
        assert_eq!(store.get(b"n").unwrap(), None);
    }

    #[test]
    fn step_counter() {
        let mut store = WriteCache::new();
        assert_eq!(step_atomic(counter, &mut store, 0).unwrap(), 1);
        assert!(step_atomic(counter, &mut store, 0).is_err());
        assert_eq!(step_atomic(counter, &mut store, 1).unwrap(), 2);
        assert!(step_atomic(counter, &mut store, 1).is_err());
        assert_eq!(store.get(b"n").unwrap(), Some(vec![1]));
        assert_eq!(store.get(b"count").unwrap(), Some(vec![2]));
    }

    #[test]
    fn closure_sm() {
        let mut store = WriteCache::new();
        assert_eq!(
            step_atomic(|_, input| Ok(input + 1), &mut store, 100).unwrap(),
            101
        );
    }
}
