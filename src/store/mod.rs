use std::borrow::{Borrow, BorrowMut};
use crate::error::Result;

mod error;
mod write_cache;
mod nullstore;
mod rwlog;
mod splitter;

pub use write_cache::WriteCache;
pub use nullstore::{NullStore, StaticNullStore};
pub use error::{Error, ErrorKind};
pub use splitter::Splitter;

// TODO: iter method?
// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

pub trait Read: Sized {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>>;

    fn as_read_ref<'a>(&'a self) -> ReadRef<'a, Self> {
        ReadRef(self)
    }
}

pub trait Write: Sized {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()>;
}

pub trait Store: Read + Write {
    fn as_store_ref<'a>(&'a mut self) -> StoreRef<'a, Self> {
        StoreRef(self)
    }
}

impl<S: Read + Write> Store for S {}

pub trait Flush {
    // TODO: should this consume the store? or will we want it like this so we
    // can persist the same wrapper store and flush it multiple times?
    fn flush(&mut self) -> Result<()>;
}

pub struct ReadRef<'a, T: Read> (&'a T);

pub struct StoreRef<'a, T: Store> (&'a mut T);

impl<'a, T: Store> std::ops::Deref for StoreRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a, T: Store> std::ops::DerefMut for StoreRef<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<'a, T: Store> Read for StoreRef<'a, T> {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>> {
        self.0.get(key)
    }
}

impl<'a, T: Store> Write for StoreRef<'a, T> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.0.put(key, value)
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()> {
        self.0.delete(key)
    }
}

// impl<T: Read> Read for &T {
//     fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>> {
//         (*self).get(key)
//     }
// }

// impl<T: Read> Read for &mut T {
//     fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>> {
//         (**self).get(key)
//     }
// }

// impl<T: Write> Write for &mut T {
//     fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
//         (*self).put(key, value)
//     }

//     fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()> {
//         (*self).delete(key)
//     }
// }

#[cfg(test)]
mod tests {
    use super::{Read, NullStore};
        
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
    fn vec_key() {
        let key = vec![1, 2, 3, 4];
        NullStore.get(key).unwrap();
    }
}
