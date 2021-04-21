use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ops::{RangeInclusive, Bound};
use std::rc::Rc;

use crate::Result;
use crate::store;
use super::MerkStore;
use merk::proofs::Query;

/// Records reads to a `MerkStore` and uses them to build a proof including all
/// accessed keys.
pub struct ProofBuilder<'a> {
    store: &'a MerkStore<'a>,
    query: Rc<RefCell<Query>>,
}

impl<'a> ProofBuilder<'a> {
    /// Constructs a `ProofBuilder` which provides read access to data in the
    /// given `MerkStore`.
    pub fn new(store: &'a MerkStore<'a>) -> Self {
        ProofBuilder {
            store,
            query: Rc::new(RefCell::new(Query::new()))
        }
    }
 
    /// Builds a Merk proof including all the data accessed during the life of
    /// the `ProofBuilder`.
    pub fn build(self) -> Result<Vec<u8>> {
        let query = self.query.take();
        self.store.merk().prove(query)
    }
}

impl<'a> store::Read for ProofBuilder<'a> {
    /// Gets the value from the underlying store, recording the key to be
    /// included in the proof when `build` is called.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.query
            .borrow_mut()
            .insert_key(key.to_vec());

        self.store.get(key)
    }
}

impl<'a> store::Iter for ProofBuilder<'a> {
    type Iter<'b> = Iter<'b>;

    fn iter_from(&self, start: &[u8]) -> Iter {
        Iter {
            bounds: (start.to_vec(), start.to_vec()),
            inner: self.store.iter_from(start),
            query: self.query.clone(),
        }
    }
}

pub struct Iter<'a> {
    bounds: (Vec<u8>, Vec<u8>),
    inner: <MerkStore<'a> as store::Iter>::Iter<'a>,
    query: Rc<RefCell<Query>>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = store::Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.inner.next();

        if let Some((key, _)) = entry {
            self.bounds.1 = key.to_vec();
        }

        entry
    }
}

impl<'a> Drop for Iter<'a> {
    fn drop(&mut self) {
        // TODO: support inserting `(Bound, Bound)` into query
        let range = self.bounds.0.clone()..=self.bounds.1.clone();
        self.query
            .borrow_mut()
            .insert_range_inclusive(range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::*;
    use crate::store::*;
    use merk::test_utils::TempMerk;
    use merk::verify_proof;

    #[test]
    fn simple() {
        let mut merk = TempMerk::new().unwrap();
        let mut store = MerkStore::new(&mut merk);
        store.put(vec![1, 2, 3], vec![2]).unwrap();
        store.put(vec![3, 4, 5], vec![4]).unwrap();
        store.write(vec![]).unwrap();

        let builder = ProofBuilder::new(&store);
        let key = [1, 2, 3];
        assert_eq!(builder.get(&key[..]).unwrap(), Some(vec![2]));
    
        let proof = builder.build().unwrap();
        let root_hash = merk.root_hash();
        let res = verify_proof(proof.as_slice(), &[vec![1, 2, 3]], root_hash).unwrap();

        assert_eq!(res[0], Some(vec![2]));
    }

    #[test]
    fn absence() {
        let mut merk = TempMerk::new().unwrap();
        let mut store = MerkStore::new(&mut merk);
        store.put(vec![1, 2, 3], vec![2]).unwrap();
        store.put(vec![3, 4, 5], vec![4]).unwrap();
        store.write(vec![]).unwrap();

        let builder = ProofBuilder::new(&store);
        let key = [5];
        assert_eq!(builder.get(&key[..]).unwrap(), None);
    
        let proof = builder.build().unwrap();
        let root_hash = merk.root_hash();
        let res = verify_proof(proof.as_slice(), &[vec![5]], root_hash).unwrap();

        assert_eq!(res[0], None);
    }
}
