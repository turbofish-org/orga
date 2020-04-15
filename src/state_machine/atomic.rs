use crate::error::Result;
use crate::store::{Flush, Store, WriteCache};

pub fn step_atomic<F, S, I, O>(sm: F, store: S, input: I) -> Result<O>
where
    S: Store,
    F: Fn(&mut WriteCache<S>, I) -> Result<O>,
{
    let mut flush_store = WriteCache::wrap(store);
    let res = sm(&mut flush_store, input)?;
    flush_store.flush()?;
    Ok(res)
}

pub fn atomize<F, S, I, O>(sm: F) -> impl Fn(S, I) -> Result<O>
where
    S: Store,
    F: Fn(&mut WriteCache<S>, I) -> Result<O>,
{
    move |store, input| step_atomic(&sm, store, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Read, Write, WriteCache};
    use failure::bail;

    fn get_u8<R: Read>(key: &[u8], store: Read) -> Result<u8> {
        match store.get(key) {
            Ok(None) => Ok(0),
            Ok(Some(vec)) => Ok(vec[0]),
            Err(err) => Err(err),
        }
    }

    fn put_u8<W: Write>(key: &[u8], value: u8, store: Write) -> Result<()> {
        store.put(key.to_vec(), vec![value])
    }

    fn counter(store: &mut Store, n: u8) -> Result<u8> {
        // put to n before checking count (to test atomicity)
        put_u8(b"n", n, store)?;

        // get count, compare to n, write if successful
        let count = get_u8(b"count", store)?;
        if count != n {
            bail!("Invalid count");
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

    #[test]
    fn atomize_counter() {
        let mut store = WriteCache::new();
        let atomic_counter = atomize(counter);

        assert_eq!(atomic_counter(&mut store, 0).unwrap(), 1);
        assert!(atomic_counter(&mut store, 0).is_err());
        assert_eq!(atomic_counter(&mut store, 1).unwrap(), 2);
        assert!(atomic_counter(&mut store, 1).is_err());
        assert_eq!(store.get(b"n").unwrap(), Some(vec![1]));
        assert_eq!(store.get(b"count").unwrap(), Some(vec![2]));
    }
}
