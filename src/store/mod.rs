use crate::error::Result;

mod nullstore;
mod rwlog;
pub mod split;
mod write_cache;

pub use nullstore::NullStore;
pub use rwlog::RWLog;
pub use split::Splitter;
pub use write_cache::Map as WriteCacheMap;
pub use write_cache::{MapStore, WriteCache};

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

impl<S: Store> Read for &S {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        (**self).get(key)
    }
}

impl<S: Store> Read for &mut S {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        (**self).get(key)
    }
}

impl<S: Store> Write for &mut S {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        (**self).put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        (**self).delete(key)
    }
}

impl<S: Store> Read for Box<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        (**self).get(key)
    }
}

impl<S: Store> Write for Box<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        (**self).put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        (**self).delete(key)
    }
}

pub trait Flush {
    // TODO: should this consume the store? or will we want it like this so we
    // can persist the same wrapper store and flush it multiple times?
    fn flush(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::{NullStore, Read};

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
