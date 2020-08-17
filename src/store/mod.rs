use crate::error::Result;
use crate::state::State;
use std::ops::{Deref, DerefMut};

mod bufstore;
mod iter;
mod nullstore;
mod prefix;
mod rwlog;
mod share;
mod split;

pub use iter::{Entry, Iter};
pub use nullstore::NullStore;
pub use prefix::Prefixed;
pub use rwlog::RWLog;
pub use share::Shared;
pub use split::Splitter;
pub use bufstore::Map as BufStoreMap;
pub use bufstore::{MapStore, BufStore};

// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

/// Trait for read access to key/value stores.
pub trait Read: Sized {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

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
}

/// Trait for write access to key/value stores.
pub trait Write {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete(&mut self, key: &[u8]) -> Result<()>;

    fn as_mut<'a>(&'a mut self) -> &'a mut Self {
        self
    }
}

/// Trait for key/value stores, automatically implemented for any type which has
/// both `Read` and `Write`.
pub trait Store: Read + Write {}

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

/// A trait for types which contain data that can be flushed to an underlying
/// store.
pub trait Flush {
    // TODO: should this consume the store? or will we want it like this so we
    // can persist the same wrapper store and flush it multiple times?
    fn flush(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::{NullStore, Read, Store};
    use crate::state::Value;

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
