use std::rc::Rc;
use std::cell::RefCell;
use super::{Store, Read, Write};
use crate::Result;

// TODO: can we do this without copying every time we prefix the key? can
// possibly change Store methods to generically support iterator-based
// concatenated keys, maybe via a Key type.

// TODO: we can probably use UnsafeCell instead of RefCell since
// operations are guaranteed not to interfere with each other

pub struct Splitter<S: Store> {
    store: Rc<RefCell<S>>,
    index: u8
}

pub struct Substore<S: Store> {
    store: Rc<RefCell<S>>,
    index: u8
}

impl<S: Store> Splitter<S> {
    pub fn new(store: S) -> Self {
        Splitter {
            store: Rc::new(RefCell::new(store)),
            index: 0
        }
    }

    pub fn split(&mut self) -> Substore<S> {
        if self.index == 255 {
            panic!("Reached split limit");
        }
        
        let index = self.index;
        self.index += 1;

        Substore {
            store: self.store.clone(),
            index
        }
    }
}

// TODO: make a Key type which doesn't need copying
#[inline]
fn prefix(prefix: u8, suffix: &[u8]) -> [u8; 256] {
    let mut prefixed = [0; 256];
    prefixed[0] = prefix;
    prefixed[1..suffix.len() + 1].copy_from_slice(suffix);
    prefixed
}

impl<S: Store> Read for Substore<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let len = key.len() + 1;
        let prefixed_key = prefix(self.index, key);

        let store = &self.store.borrow();
        store.get(&prefixed_key[..len])
    }
}

impl<S: Store> Write for Substore<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let len = key.len() + 1;
        let prefixed_key = prefix(self.index, key.as_slice())[..len].to_vec();

        let store = &mut self.store.borrow_mut();
        store.put(prefixed_key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let len = key.as_ref().len() + 1;
        let prefixed_key = prefix(self.index, key);

        let store = &mut self.store.borrow_mut();
        store.delete(&prefixed_key[..len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MapStore, Read, Write};

    #[test]
    fn split() {
        let mut store = MapStore::new();

        let mut splitter = Splitter::new(&mut store);
        let mut sub0 = splitter.split();
        let mut sub1 = splitter.split();

        sub0.put(vec![123], vec![5]).unwrap();
        assert_eq!(sub0.get(&[123]).unwrap(), Some(vec![5]));
        assert_eq!(sub1.get(&[123]).unwrap(), None);

        sub1.put(vec![123], vec![6]).unwrap();
        assert_eq!(sub0.get(&[123]).unwrap(), Some(vec![5]));
        assert_eq!(sub1.get(&[123]).unwrap(), Some(vec![6]));

        assert_eq!(store.get(&[0, 123]).unwrap(), Some(vec![5]));
        assert_eq!(store.get(&[1, 123]).unwrap(), Some(vec![6]));
    }
}
