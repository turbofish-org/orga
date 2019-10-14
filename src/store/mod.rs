use crate::error::Result;

mod error;
mod mapstore;

pub use mapstore::MapStore;
pub use error::{Error, ErrorKind};

// TODO: iter method?

pub trait Read {
  fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Vec<u8>>;
}

pub trait Write {
  fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

  fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()>;
}

pub trait Store: Read + Write {}

impl<S: Read + Write> Store for S {}

pub trait Flush: Write {
    fn flush(self) -> Result<()>;
}
