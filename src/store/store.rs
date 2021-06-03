use super::{Shared, Read, Write, KV};
use crate::Result;

// TODO: figure out how to let users set DefaultBackingStore, similar to setting
// the global allocator in the standard library

#[cfg(merk)]
pub type DefaultBackingStore = crate::merk::MerkStore;

#[cfg(any(test, not(merk)))]
// TODO: default to a dynamic store for non-production builds
pub type DefaultBackingStore = super::MapStore;

pub struct Store<S = DefaultBackingStore> {
    prefix: Vec<u8>,
    store: Shared<S>,
}

impl<S> Clone for Store<S> {
    fn clone(&self) -> Self {
        Store {
            prefix: self.prefix.clone(),
            store: self.store.clone(),
        }
    }
}

impl<S> Store<S> {
    #[inline]
    pub fn new<'a>(inner: S) -> Self {
        Store {
            prefix: vec![],
            store: Shared::new(inner),
        }
    }

    #[inline]
    pub fn sub(&self, key: &[u8]) -> Self {
        Store {
            prefix: concat(self.prefix.as_slice(), key),
            store: self.store.clone(),
        }
    }
}

impl<S: Read> Read for Store<S> {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let prefixed = concat(self.prefix.as_slice(), key);
        self.store.get(prefixed.as_slice())
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let prefixed = concat(self.prefix.as_slice(), key);
        self.store.get_next(prefixed.as_slice())
    }
}

impl<S: Write> Write for Store<S> {
    #[inline]
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let prefixed = concat(self.prefix.as_slice(), key.as_slice());
        self.store.put(prefixed, value)
    }

    #[inline]
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let prefixed = concat(self.prefix.as_slice(), key);
        self.store.delete(prefixed.as_slice())
    }
}

#[inline]
fn concat(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(a.len() + b.len());
    value.extend_from_slice(a);
    value.extend_from_slice(b);
    value 
}
