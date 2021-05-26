// use super::prefix::BytePrefixed;
// use super::share::Shared;
// use super::Read;

// // TODO: can we do this without copying every time we prefix the key? can
// // possibly change Store methods to generically support iterator-based
// // concatenated keys, maybe via a Key type.

// /// A store wrapper which can be used to create multiple substores, which all
// /// read from and write to the same underlying store with a unique prefix per
// /// substore.
// pub struct Splitter<S: Read> {
//     store: Shared<S>,
//     index: u8,
// }

// impl<S: Read> Splitter<S> {
//     /// Constructs a `Splitter` for the given store.
//     pub fn new(store: S) -> Self {
//         Splitter {
//             store: store.into_shared(),
//             index: 0,
//         }
//     }

//     /// Creates and returns a new substore, which is a prefixed store that
//     /// shares a reference to the `Splitter`'s inner store. Each call to `split`
//     /// will increment the internal index and create a substore with the next
//     /// value, in order from 0 to 255 (inclusive).
//     ///
//     /// `split` can not be called more than 255 times on one splitter instance.
//     pub fn split(&mut self) -> BytePrefixed<Shared<S>> {
//         if self.index == 255 {
//             panic!("Reached split limit");
//         }

//         let index = self.index;
//         self.index += 1;

//         BytePrefixed::new(self.store.clone(), index)
//     }
// }


// /// A store wrapper which can be used to create multiple substores, which all
// /// read from and write to the same underlying store with a unique prefix per
// /// substore.
// pub struct Splitter<S: Read> {
//     store: Shared<S>,
//     index: u8,
// }

// impl<S: Read> Splitter<S> {
//     /// Constructs a `Splitter` for the given store.
//     pub fn new(store: S) -> Self {
//         Splitter {
//             store: Shared::new(store),
//             index: 0,
//         }
//     }

//     /// Creates and returns a new substore, which is a prefixed store that
//     /// shares a reference to the `Splitter`'s inner store. Each call to `split`
//     /// will increment the internal index and create a substore with the next
//     /// value, in order from 0 to 255 (inclusive).
//     ///
//     /// `split` can not be called more than 255 times on one splitter instance.
//     pub fn split(&mut self) -> BytePrefixed<Shared<S>> {
//         if self.index == 255 {
//             panic!("Reached split limit");
//         }

//         let index = self.index;
//         self.index += 1;

//         BytePrefixed::new(self.store.clone(), index)
//     }
// }


// // #[cfg(test)]
// // mod tests {
// //     use super::*;
// //     use crate::store::{MapStore, Read, Write};

// //     #[test]
// //     fn split() {
// //         let mut store = MapStore::new();

// //         let mut splitter = Splitter::new(&mut store);
// //         let mut sub0 = splitter.split();
// //         let mut sub1 = splitter.split();

// //         sub0.put(vec![123], vec![5]).unwrap();
// //         assert_eq!(sub0.get(&[123]).unwrap(), Some(vec![5]));
// //         assert_eq!(sub1.get(&[123]).unwrap(), None);

// //         sub1.put(vec![123], vec![6]).unwrap();
// //         assert_eq!(sub0.get(&[123]).unwrap(), Some(vec![5]));
// //         assert_eq!(sub1.get(&[123]).unwrap(), Some(vec![6]));

// //         assert_eq!(store.get(&[0, 123]).unwrap(), Some(vec![5]));
// //         assert_eq!(store.get(&[1, 123]).unwrap(), Some(vec![6]));
// //     }
// // }
