use crate::error::Result;
use crate::state::State;
use std::ops::{Deref, DerefMut};

pub mod bufstore;
pub mod iter;
pub mod nullstore;
pub mod prefix;
pub mod rwlog;
pub mod share;
pub mod split;

pub use bufstore::Map as BufStoreMap;
pub use bufstore::{BufStore, MapStore};
pub use iter::{Entry, Iter};
pub use nullstore::NullStore;
pub use prefix::BytePrefixed;
pub use rwlog::RWLog;
pub use share::Shared;
pub use split::Splitter;

// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

/// Trait for read access to key/value stores.
pub trait Read: Sized {
    /// Gets a value by key.
    ///
    /// Implementations of `get` should return `None` when there is no value for
    /// the key rather than erroring.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Wraps self with a given [`state::State`](../state/trait.State.html)
    /// implementation, provided for convenience.
    fn wrap<T: State<Self>>(self) -> Result<T> {
        T::wrap_store(self)
    }

    /// Wraps self with [`Shared`](struct.Shared.html), allowing it to be cloned
    /// so that multiple callers can share the reference to the underlying
    /// store.
    fn into_shared(self) -> Shared<Self> {
        Shared::new(self)
    }

    /// Wraps self with [`Prefixed`](struct.Prefixed.html) using the given
    /// prefix byte, so that all operations have the prefix prepended to their
    /// keys.
    fn prefix(self, prefix: u8) -> BytePrefixed<Self> {
        BytePrefixed::new(self, prefix)
    }

    /// Wraps self with [`Splitter`](struct.Splitter.html) so that prefixed
    /// substores may be created by calling `.split()`.
    fn into_splitter(self) -> Splitter<Self> {
        Splitter::new(self)
    }

    /// Returns an immutable reference to the store.
    fn as_ref<'a>(&'a self) -> &'a Self {
        self
    }
}

pub trait Read2 {
    /// Gets a value by key.
    ///
    /// Implementations of `get` should return `None` when there is no value for
    /// the key rather than erroring.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
}

impl<R: Read2, T: Deref<Target = R>> Read2 for T {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.deref().get(key)
    }
}

/// Trait for write access to key/value stores.
pub trait Write {
    /// Writes a key and value to the store.
    ///
    /// If a value already exists for the given key, implementations should
    /// overwrite the value.
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    /// Deletes the value with the given key.
    ///
    /// If no value exists for the given key, implementations should treat the
    /// operation as a no-op (but may still issue a call to `delete` to an
    /// underlying store).
    fn delete(&mut self, key: &[u8]) -> Result<()>;

    /// Returns a mutable reference to the store.
    fn as_mut<'a>(&'a mut self) -> &'a mut Self {
        self
    }
}

/// Trait for write access to key/value stores.
pub trait Write2: Read2 {
    /// Writes a key and value to the store.
    ///
    /// If a value already exists for the given key, implementations should
    /// overwrite the value.
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    /// Deletes the value with the given key.
    ///
    /// If no value exists for the given key, implementations should treat the
    /// operation as a no-op (but may still issue a call to `delete` to an
    /// underlying store).
    fn delete(&mut self, key: &[u8]) -> Result<()>;
}

impl<W: Write2, T: std::ops::DerefMut<Target = W>> Write2 for T {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.deref_mut().put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.deref_mut().delete(key)
    }
}

pub trait Sub {
    // TODO: support substores that aren't type Self
    fn sub(&self, prefix: Vec<u8>) -> Self;
}

/// Trait for key/value stores, automatically implemented for any type which has
/// both `Read` and `Write`.
pub trait Store: Read + Write {}

impl<S: Read + Write + Sized> Store for S {}

impl<S: Read, T: Deref<Target = S>> Read for T {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.deref().get(key)
    }
}

impl<S: Write, T: DerefMut<Target = S>> Write for T {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.deref_mut().put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.deref_mut().delete(key)
    }
}

impl<'a, S: Iter + 'a> Iter for &mut S {
    type Iter<'b> = <S as Iter>::Iter<'b>;

    fn iter_from(&self, start: &[u8]) -> Self::Iter<'_> {
        self.deref().iter_from(start)
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
    use super::{NullStore, Read};
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
