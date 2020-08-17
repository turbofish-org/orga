use super::prefix::Prefixed;
use super::share::Shared;
use super::Read;

// TODO: can we do this without copying every time we prefix the key? can
// possibly change Store methods to generically support iterator-based
// concatenated keys, maybe via a Key type.

/// A store wrapper which can be used to create multiple substores, which all
/// read from and write to the same underlying store with a unique prefix per
/// substore.
pub struct Splitter<S: Read> {
    store: Shared<S>,
    index: u8,
}

impl<S: Read> Splitter<S> {
    pub fn new(store: S) -> Self {
        Splitter {
            store: store.into_shared(),
            index: 0,
        }
    }

    pub fn split(&mut self) -> Prefixed<Shared<S>> {
        if self.index == 255 {
            panic!("Reached split limit");
        }

        let index = self.index;
        self.index += 1;

        self.store.clone().prefix(index)
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
    use crate::store::{MapStore, Read, Write};

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
