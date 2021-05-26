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
// pub use prefix::BytePrefixed;
pub use rwlog::RWLog;
pub use share::Shared;
// pub use split::Splitter;

// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

/// Trait for read access to key/value stores.
pub trait Read {
    /// Gets a value by key.
    ///
    /// Implementations of `get` should return `None` when there is no value for
    /// the key rather than erroring.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
}

impl<R: Read, T: Deref<Target = R>> Read for T {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.deref().get(key)
    }
}

/// Trait for write access to key/value stores.
pub trait Write: Read {
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

impl<S: Write, T: DerefMut<Target = S>> Write for T {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.deref_mut().put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.deref_mut().delete(key)
    }
}

pub trait ReadWrite {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
   
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete(&mut self, key: &[u8]) -> Result<()>;
}

impl<T: Read + Write> ReadWrite for T {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Read::get(self, key)
    }
   
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        Write::put(self, key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        Write::delete(self, key)
    }
}

pub struct ReadWriter(pub Box<dyn ReadWrite>);

impl<'a> Read for ReadWriter {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        ReadWrite::get(self.0.deref(), key)
    }
}

impl<'a> Write for ReadWriter {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        ReadWrite::put(self.0.deref_mut(), key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        ReadWrite::delete(self.0.deref_mut(), key)
    }
}

pub trait Sub {
    // TODO: support substores that aren't type Self
    fn sub(&self, prefix: Vec<u8>) -> Self;
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

// #[cfg(test)]
// mod tests {
//     use super::{NullStore, Read};
//     use crate::state::Value;

//     #[test]
//     fn fixed_length_slice_key() {
//         let key = b"0123";
//         NullStore.get(key).unwrap();
//     }

//     #[test]
//     fn slice_key() {
//         let key = vec![1, 2, 3, 4];
//         NullStore.get(key.as_slice()).unwrap();
//     }

//     #[test]
//     fn wrap() {
//         let _: Value<_, u64> = NullStore.wrap().unwrap();
//     }
// }
