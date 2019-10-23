use std::cell::{RefCell, Ref, RefMut};
use super::*;

pub struct Splitter<S: Store> {
    store: RefCell<S>,
    index: u8
}

pub struct Substore<'a, S: Store> {
    parent: &'a Splitter<S>,
    index: u8
}

impl<S: Store> Splitter<S> {
    pub fn new(store: S) -> Self {
        Splitter {
            store: RefCell::new(store),
            index: 0
        }
    }

    pub fn split(&mut self) -> Substore<'_, S> {
        if self.index == 255 {
            panic!("Splitter split too many times");
        }
        
        let index = self.index;
        self.index += 1;

        Substore { parent: self, index }
    }
}

impl<'a, S: Store> Substore<'a, S> {
    fn store(&self) -> Ref<S> {
        self.parent.store.borrow()
    }

    fn store_mut(&mut self) -> RefMut<S> {
        self.parent.store.borrow_mut()
    }
}

#[inline]
fn prefix<K: AsRef<[u8]>>(prefix: u8, suffix: K) -> [u8; 256] {
    let mut prefixed = [0; 256];
    prefixed[0] = prefix;
    prefixed[1..].copy_from_slice(suffix.as_ref());
    prefixed
}

impl<'a, S: Store> Read for Substore<'a, S> {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>> {
        let len = key.as_ref().len() + 1;
        let prefixed_key = prefix(self.index, key);
        self.store().get(&prefixed_key[..len])
    }
}

impl<'a, S: Store> Write for Substore<'a, S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let len = key.len() + 1;
        let prefixed_key = prefix(self.index, key)[..len].to_vec();
        self.store_mut().put(prefixed_key, value)
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()> {
        let len = key.as_ref().len() + 1;
        let prefixed_key = prefix(self.index, key);
        self.store_mut().delete(&prefixed_key[..len])
    }
}
