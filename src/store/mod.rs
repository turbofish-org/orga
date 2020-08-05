use crate::error::Result;
use crate::state::State;
use std::ops::{Deref, DerefMut};

mod iter;
mod nullstore;
mod prefix;
mod rwlog;
pub mod share;
pub mod split;
mod write_cache;

pub use iter::{Entry, Iter};
pub use nullstore::NullStore;
pub use prefix::Prefixed;
pub use rwlog::RWLog;
pub use share::Shared;
pub use split::Splitter;
pub use write_cache::Map as WriteCacheMap;
pub use write_cache::{MapStore, WriteCache};

// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

pub trait Read {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
}

pub trait Write {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete(&mut self, key: &[u8]) -> Result<()>;
}

pub trait Store: Read + Write + Sized {
    fn wrap<T: State<Self>>(self) -> Result<T> {
        T::wrap_store(self)
    }

    fn into_shared(self) -> Shared<Self> {
        Shared::new(self)
    }

    fn prefix(self, prefix: u8) -> Prefixed<Self> {
        Prefixed::new(self, prefix)
    }

    fn into_splitter(self) -> Splitter<Self> {
        Splitter::new(self)
    }

    fn as_ref<'a>(&'a self) -> &'a Self {
        self
    }

    fn as_mut<'a>(&'a mut self) -> &'a mut Self {
        self
    }
}

impl<S: Read + Write + Sized> Store for S {}

impl<S: Store, T: Deref<Target = S>> Read for T {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.deref().get(key)
    }
}

impl<S: Store, T: DerefMut<Target = S>> Write for T {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.deref_mut().put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.deref_mut().delete(key)
    }
}

pub trait Flush {
    // TODO: should this consume the store? or will we want it like this so we
    // can persist the same wrapper store and flush it multiple times?
    fn flush(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::{MapStore, NullStore, Read, Store};
    use crate::Value;

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

    #[test]
    fn wrap() {
        let _: Value<_, u64> = NullStore.wrap().unwrap();
    }
}
