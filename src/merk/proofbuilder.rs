#[cfg(test)]
use mutagen::mutate;

use std::cell::RefCell;
use std::rc::Rc;

use super::MerkStore;
use crate::store;
use crate::store::Shared;
use crate::Result;
use merk::proofs::query::Query;

/// Records reads to a `MerkStore` and uses them to build a proof including all
/// accessed keys.
#[derive(Clone)]
pub struct ProofBuilder {
    store: Shared<MerkStore>,
    query: Rc<RefCell<Query>>,
}

impl ProofBuilder {
    /// Constructs a `ProofBuilder` which provides read access to data in the
    /// given `MerkStore`.
    #[cfg_attr(test, mutate)]
    pub fn new(store: Shared<MerkStore>) -> Self {
        ProofBuilder {
            store,
            query: Rc::new(RefCell::new(Query::new())),
        }
    }

    /// Builds a Merk proof including all the data accessed during the life of
    /// the `ProofBuilder`.
    #[cfg_attr(test, mutate)]
    pub fn build(self) -> Result<Vec<u8>> {
        let store = self.store.borrow();
        let query = self.query.take();

        Ok(store.merk().prove(query)?)
    }
}

impl store::Read for ProofBuilder {
    /// Gets the value from the underlying store, recording the key to be
    /// included in the proof when `build` is called.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.query.borrow_mut().insert_key(key.to_vec());

        self.store.get(key)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<store::KV>> {
        let mut maybe_next_key = None;
        let maybe_entry = self.store.get_next(key)?.map(|(next_key, value)| {
            // TODO: support inserting `(Bound, Bound)` into query
            maybe_next_key = Some(next_key.clone());

            (next_key.to_vec(), value.to_vec())
        });
        let range = match maybe_next_key {
            Some(next_key) => key.to_vec()..=next_key.to_vec(),
            None => key.to_vec()..=key.to_vec(),
        };

        self.query.borrow_mut().insert_range_inclusive(range);
        Ok(maybe_entry)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::MerkStore;
    use super::*;
    use crate::store::*;
    use merk::proofs::query::verify;
    use tempdir::TempDir;

    fn temp_merk_store() -> MerkStore {
        let temp_dir = TempDir::new("TempMerkStore").unwrap();
        MerkStore::new(temp_dir.path().into())
    }

    #[test]
    fn simple() {
        let mut store = Shared::new(temp_merk_store());
        store.put(vec![1, 2, 3], vec![2]).unwrap();
        store.put(vec![3, 4, 5], vec![4]).unwrap();
        store.borrow_mut().write(vec![]).unwrap();

        let builder = ProofBuilder::new(store.clone());
        let key = [1, 2, 3];
        assert_eq!(builder.get(&key[..]).unwrap(), Some(vec![2]));

        let proof = builder.build().unwrap();
        let root_hash = store.borrow().merk().root_hash();
        let map = verify(proof.as_slice(), root_hash).unwrap();
        let res = map.get(&[1, 2, 3]).unwrap();
        assert_eq!(res, Some(&[2][..]));
    }

    #[test]
    fn absence() {
        let mut store = Shared::new(temp_merk_store());
        store.put(vec![1, 2, 3], vec![2]).unwrap();
        store.put(vec![3, 4, 5], vec![4]).unwrap();
        store.borrow_mut().write(vec![]).unwrap();

        let builder = ProofBuilder::new(store.clone());
        let key = [5];
        assert_eq!(builder.get(&key[..]).unwrap(), None);

        let proof = builder.build().unwrap();
        let root_hash = store.borrow().merk().root_hash();
        let map = verify(proof.as_slice(), root_hash).unwrap();
        let res = map.get(&[5]).unwrap();

        assert_eq!(res, None);
    }

    #[test]
    fn simple_get_next() {
        let mut store = Shared::new(temp_merk_store());
        store.put(vec![1, 2, 3], vec![2]).unwrap();
        store.put(vec![3, 4, 5], vec![4]).unwrap();
        store.borrow_mut().write(vec![]).unwrap();

        let builder = ProofBuilder::new(store.clone());
        let key = [3, 4, 4];
        assert_eq!(
            builder.get_next(&key[..]).unwrap(),
            Some((vec![3, 4, 5], vec![4]))
        );

        let proof = builder.build().unwrap();
        let root_hash = store.borrow().merk().root_hash();
        let map = verify(proof.as_slice(), root_hash).unwrap();
        let mut iter = map.range(&[3, 4, 4][..]..=&[3, 4, 5][..]);

        let res = iter.next().unwrap().unwrap();

        assert_eq!(res, (&[3, 4, 5][..], &[4][..]));
    }

    #[test]
    fn none_get_next() {
        let mut store = Shared::new(temp_merk_store());
        store.put(vec![1, 2, 3], vec![2]).unwrap();
        store.put(vec![3, 4, 5], vec![4]).unwrap();
        store.borrow_mut().write(vec![]).unwrap();

        let builder = ProofBuilder::new(store.clone());
        let key = [3, 4, 5];
        assert_eq!(builder.get_next(&key[..]).unwrap(), None);

        let proof = builder.build().unwrap();
        let root_hash = store.borrow().merk().root_hash();
        let map = verify(proof.as_slice(), root_hash).unwrap();
        assert!(map.get(&[3, 4, 5]).unwrap().is_some());
        let mut iter = map.range(&[3, 4, 5][..]..=&[3, 4, 7][..]);

        let _res = iter.next().unwrap().unwrap();
        //assert!(res.is_none());
    }
}
