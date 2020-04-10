use std::ops::{Deref, DerefMut};
use crate::error::Result;

mod write_cache;
mod nullstore;
mod rwlog;
mod splitter;

pub use write_cache::{WriteCache, MapStore};
pub use write_cache::Map as WriteCacheMap;
pub use nullstore::{NullStore, NULL_STORE};
pub use splitter::Splitter;
pub use rwlog::RWLog;

// TODO: iter method?
// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

pub trait Read {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
}

pub trait Write {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete(&mut self, key: &[u8]) -> Result<()>;
}

pub trait Store: Read + Write {
    fn as_read(&self) -> &dyn Read;

    fn as_write(&mut self) -> &mut dyn Write;
}

impl<S: Read + Write + Sized> Store for S {
    fn as_read(&self) -> &dyn Read {
        self
    }

    fn as_write(&mut self) -> &mut dyn Write {
        self
    }
}

pub trait Flush {
    // TODO: should this consume the store? or will we want it like this so we
    // can persist the same wrapper store and flush it multiple times?
    fn flush(&mut self) -> Result<()>;
}

pub trait WrapStore<'a> {
    fn wrap_store(store: &'a mut dyn Store) -> Self;
}

#[cfg(test)]
mod tests {
    use std::ops::Add;
    use std::marker::PhantomData;
    use super::{Read, NullStore, WrapStore, Store, Result};
    use crate::{Encode, Decode};

    const EMPTY_KEY: &[u8] = &[];

    struct Counter<'a, T>
        where T:
            Add<T, Output = T> +
            From<u8> +
            Encode +
            Decode +
            Default
    {
        store: &'a mut dyn Store,
        number_type: PhantomData<T>
    }

    impl<'a, T> Counter<'a, T>
        where T:
            Add<T, Output = T> +
            From<u8> +
            Encode +
            Decode +
            Default
    {
        fn get(&self) -> Result<T> {
            Ok(self.store.get(EMPTY_KEY)?
                .map(|bytes| T::decode(bytes.as_slice()))
                .transpose()?
                .unwrap_or_default())
        }

        fn increment(&mut self) -> Result<()> {
            let value = self.get()?;
            let value = value + T::from(1u8);
            let bytes = value.encode()?;
            self.store.put(EMPTY_KEY.to_vec(), bytes)
        }
    }

    impl<'a, T: Add<T>> WrapStore<'a> for Counter<'a, T>
        where T:
            Add<T, Output = T> +
            From<u8> +
            Encode +
            Decode +
            Default
    {
        fn wrap_store(store: &'a mut dyn Store) -> Self {
            Counter { store, number_type: PhantomData }
        }
    }

    #[test]
    fn wrap_store() {
        use super::MapStore;

        let mut store = MapStore::new();
        let mut counter: Counter<u64> = Counter::wrap_store(&mut store);
        assert_eq!(counter.get().unwrap(), 0);
        counter.increment().unwrap();
        assert_eq!(counter.get().unwrap(), 1);
        assert_eq!(store.get(EMPTY_KEY).unwrap(), Some(vec![0, 0, 0, 0, 0, 0, 0, 1]));
    }
        
    #[test]
    fn fixed_length_slice_key() {
        let key = b"0123";
        NullStore.get(key).unwrap();
    }

    #[test]
    fn slice_key() {
        let key = vec![1, 2, 3, 4];
        NullStore.get(key.as_slice()).unwrap();
    }
}
