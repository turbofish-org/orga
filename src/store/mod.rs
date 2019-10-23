use crate::error::Result;

mod error;
mod write_cache;
mod nullstore;
mod key_log;
mod splitter;

pub use write_cache::WriteCache;
pub use nullstore::NullStore;
pub use error::{Error, ErrorKind};
pub use splitter::Splitter;

// TODO: iter method?

pub trait Read {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>>;
}

pub trait Write {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()>;
}

pub trait Store: Read + Write {}

impl<S: Read + Write> Store for S {}

pub trait Flush {
    fn flush<S: Store>(self, dest: &mut S) -> Result<()>;
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

    #[test]
    fn vec_key() {
        let key = vec![1, 2, 3, 4];
        NullStore.get(key).unwrap();
    }
}
