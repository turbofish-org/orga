use std::sync::{RwLock, RwLockReadGuard};

use crate::Result;

use super::{Read, Write, KV};

pub struct ReadLog<T> {
    inner: T,
    reads: RwLock<Vec<Vec<u8>>>,
}

impl<T> ReadLog<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            reads: RwLock::new(Vec::new()),
        }
    }

    pub fn reads(&self) -> RwLockReadGuard<Vec<Vec<u8>>> {
        self.reads.read().unwrap()
    }
}

impl<T: Read> Read for ReadLog<T> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.reads.write().unwrap().push(key.to_vec());
        self.inner.get(key)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        self.reads.write().unwrap().push(key.to_vec());
        self.inner.get_next(key)
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        // TODO: handle None
        self.reads.write().unwrap().push(key.unwrap().to_vec());
        self.inner.get_prev(key)
    }
}

impl<T: Write> Write for ReadLog<T> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.inner.put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.inner.delete(key)
    }
}
