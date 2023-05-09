use std::cell::{Ref, RefCell};

use crate::Result;

use super::{Read, Write, KV};

pub struct ReadLog<T> {
    inner: T,
    reads: RefCell<Vec<Vec<u8>>>,
}

impl<T> ReadLog<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            reads: RefCell::new(Vec::new()),
        }
    }

    pub fn reads(&self) -> Ref<Vec<Vec<u8>>> {
        self.reads.borrow()
    }
}

impl<T: Read> Read for ReadLog<T> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.reads.borrow_mut().push(key.to_vec());
        self.inner.get(key)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        self.reads.borrow_mut().push(key.to_vec());
        self.inner.get_next(key)
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
