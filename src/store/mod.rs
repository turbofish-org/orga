use crate::error::Result;

mod error;
mod write_cache;
mod nullstore;
mod rwlog;
mod splitter;

pub use write_cache::WriteCache;
pub use nullstore::{NullStore, NULL_STORE};
pub use error::{Error, ErrorKind};
pub use splitter::Splitter;

// TODO: iter method?
// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

pub trait Read {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
}

pub trait Write {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete(&mut self, key: &[u8]) -> Result<()>;
}

pub trait Store: Read + Write {}

impl<S: Read + Write> Store for S {}

pub trait Flush {
    // TODO: should this consume the store? or will we want it like this so we
    // can persist the same wrapper store and flush it multiple times?
    fn flush(&mut self) -> Result<()>;
}

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
}
