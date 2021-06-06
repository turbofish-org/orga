// use std::cell::RefCell;
// use std::collections::BTreeSet;
// use std::ops::{RangeInclusive, Bound};
// use std::rc::Rc;

// use crate::Result;
// use crate::store;
// use super::MerkStore;

// /// Records reads to a `MerkStore` and uses them to build a proof including all
// /// accessed keys.
pub struct ProofBuilder {
    // store: &'a MerkStore<'a>,
    // query: Rc<RefCell<Query>>,
}

// impl<'a> ProofBuilder<'a> {
//     /// Constructs a `ProofBuilder` which provides read access to data in the
//     /// given `MerkStore`.
//     pub fn new(store: &'a MerkStore<'a>) -> Self {
//         ProofBuilder {
//             store,
//             query: Rc::new(RefCell::new(Query::new()))
//         }
//     }
 
//     /// Builds a Merk proof including all the data accessed during the life of
//     /// the `ProofBuilder`.
//     pub fn build(self) -> Result<Vec<u8>> {
//         let query = self.query.take();
//         self.store.merk().prove(query)
//     }
// }

// impl<'a> store::Read for ProofBuilder<'a> {
//     /// Gets the value from the underlying store, recording the key to be
//     /// included in the proof when `build` is called.
//     fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
//         self.query
//             .borrow_mut()
//             .insert_key(key.to_vec());

//         self.store.get(key)
//     }

//     fn get_next(&self, key: &[u8]) -> Result<Option<store::KV>> {
//         let maybe_entry = self.store
//             .get_next(key)?
//             .map(|(next_key, value)| {
//                 // TODO: support inserting `(Bound, Bound)` into query
//                 let range = key.to_vec()..=next_key.to_vec();
//                 self.query
//                     .borrow_mut()
//                     .insert_range_inclusive(range);

//                 (next_key.to_vec(), value.to_vec())
//             });
//         Ok(maybe_entry)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use super::super::*;
//     use crate::store::*;
//     use merk::test_utils::TempMerk;
//     use merk::verify_proof;

//     #[test]
//     fn simple() {
//         let mut merk = TempMerk::new().unwrap();
//         let mut store = MerkStore::new(&mut merk);
//         store.put(vec![1, 2, 3], vec![2]).unwrap();
//         store.put(vec![3, 4, 5], vec![4]).unwrap();
//         store.write(vec![]).unwrap();

//         let builder = ProofBuilder::new(&store);
//         let key = [1, 2, 3];
//         assert_eq!(builder.get(&key[..]).unwrap(), Some(vec![2]));
    
//         let proof = builder.build().unwrap();
//         let root_hash = merk.root_hash();
//         let res = verify_proof(proof.as_slice(), &[vec![1, 2, 3]], root_hash).unwrap();

//         assert_eq!(res[0], Some(vec![2]));
//     }

//     #[test]
//     fn absence() {
//         let mut merk = TempMerk::new().unwrap();
//         let mut store = MerkStore::new(&mut merk);
//         store.put(vec![1, 2, 3], vec![2]).unwrap();
//         store.put(vec![3, 4, 5], vec![4]).unwrap();
//         store.write(vec![]).unwrap();

//         let builder = ProofBuilder::new(&store);
//         let key = [5];
//         assert_eq!(builder.get(&key[..]).unwrap(), None);
    
//         let proof = builder.build().unwrap();
//         let root_hash = merk.root_hash();
//         let res = verify_proof(proof.as_slice(), &[vec![5]], root_hash).unwrap();

//         assert_eq!(res[0], None);
//     }
// }
