use super::{Read, Store, Write};
use crate::Result;
use std::cell::RefCell;
use std::rc::Rc;

// TODO: can we do this without copying every time we prefix the key? can
// possibly change Store methods to generically support iterator-based
// concatenated keys, maybe via a Key type.

// TODO: we can probably use UnsafeCell instead of RefCell since
// operations are guaranteed not to interfere with each other

pub struct Splitter<S: Store> {
    store: Rc<RefCell<S>>,
    index: u8,
}

pub struct Substore<S> {
    store: Rc<RefCell<S>>,
    index: u8,
}

impl<S: Store> Splitter<S> {
    pub fn new(store: S) -> Self {
        Splitter {
            store: Rc::new(RefCell::new(store)),
            index: 0,
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
            index,
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

// impl<'a, 'b: 'a> super::Iter<'a, 'b> for Substore<super::WriteCache<super::NullStore>> {
//     type Iter = Iter<'a, 'b, <super::WriteCache<super::NullStore> as Iter<'a, 'b>>::Iter>;

//     fn iter_from(&'a self, start: &[u8]) -> Self::Iter {
//         let mut key = Vec::with_capacity(start.len() + 1);
//         key.push(self.index);
//         key.extend(start);

//         Iter {
//             store: self.store.clone(),
//             key: Some(key),
//             index: self.index,
//             phantom_a: std::marker::PhantomData,
//             phantom_b: std::marker::PhantomData,
//         }
//     }
// }

// pub struct Iter<'a, 'b: 'a, S>
// where
//     S: super::Iter<'a, 'b>,
// {
//     store: Rc<RefCell<S>>,
//     key: Option<Vec<u8>>,
//     index: u8,
//     phantom_a: std::marker::PhantomData<&'a u8>,
//     phantom_b: std::marker::PhantomData<&'b u8>,
// }

// impl<'a, 'b: 'a, S> Iterator for Iter<'a, 'b, S>
// where
//     S: super::Iter<'a, 'b>,
// {
//     type Item = (&'b [u8], &'b [u8]);

//     fn next(&mut self) -> Option<Self::Item> {
//         if self.key.is_none() {
//             return None;
//         }

//         let mut key = self.key.as_mut().unwrap();
//         let store = self.store.borrow();
//         let mut iter = store.iter_from(key.as_slice());

//         let maybe_entry = iter.next();
//         iter.next().map(|(next_key, _)| {
//             if next_key[0] != self.index {
//                 self.key.take();
//             } else {
//                 key.resize(1, 0);
//                 key.extend(next_key);
//             }
//         });

//         match maybe_entry {
//             None => None,
//             Some((key, value)) if key[0] > self.index => None,
//             Some((key, value)) => Some((&key[1..], value)),
//         }
//     }
// }

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
