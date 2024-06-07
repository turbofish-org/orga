use std::sync::{Arc, RwLock};

use super::memsnapshot::MemSnapshot;
use super::snapshot::Snapshot;
use super::MerkStore;
use crate::store::Shared;
use crate::store::{self};
use crate::Result;
use merk::proofs::query::Query;
use store::Read;

/// Records reads to a `MerkStore` and uses them to build a proof including all
/// accessed keys.
pub struct ProofBuilder<T> {
    store: Shared<T>,
    query: Arc<RwLock<Query>>,
}

impl<T> Clone for ProofBuilder<T> {
    fn clone(&self) -> Self {
        ProofBuilder {
            store: self.store.clone(),
            query: self.query.clone(),
        }
    }
}

impl<T: Prove> ProofBuilder<T> {
    /// Constructs a `ProofBuilder` which provides read access to data in the
    /// given `MerkStore`.
    pub fn new(store: Shared<T>) -> Self {
        ProofBuilder {
            store,
            query: Arc::new(RwLock::new(Query::new())),
        }
    }

    /// Builds a Merk proof including all the data accessed during the life of
    /// the `ProofBuilder`.
    pub fn build(self) -> Result<(Vec<u8>, Shared<T>)> {
        let store = self.store.borrow();
        let query = Arc::into_inner(self.query).unwrap().into_inner().unwrap();
        let proof = store.prove(query)?;
        drop(store);
        Ok((proof, self.store))
    }
}

impl<T: Read> store::Read for ProofBuilder<T> {
    /// Gets the value from the underlying store, recording the key to be
    /// included in the proof when `build` is called.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.query.write().unwrap().insert_key(key.to_vec());

        self.store.get(key)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<store::KV>> {
        let mut maybe_next_key = None;
        let maybe_entry = self.store.get_next(key)?.map(|(next_key, value)| {
            maybe_next_key = Some(next_key.clone());
            (next_key.to_vec(), value.to_vec())
        });

        // TODO: support inserting `(Bound, Bound)` into query
        let range = match maybe_next_key {
            Some(next_key) => key.to_vec()..=next_key.to_vec(),
            None => key.to_vec()..=key.to_vec(),
        };

        self.query.write().unwrap().insert_range_inclusive(range);
        Ok(maybe_entry)
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<store::KV>> {
        let mut maybe_prev_key = None;
        let maybe_entry = self.store.get_prev(key)?.map(|(prev_key, value)| {
            maybe_prev_key = Some(prev_key.clone());
            (prev_key.to_vec(), value.to_vec())
        });

        // TODO: support inserting `(Bound, Bound)` into query
        let mut query = self.query.write().unwrap();
        match key {
            Some(key) => match maybe_prev_key {
                Some(prev_key) => query.insert_range(prev_key.to_vec()..key.to_vec()),
                None => query.insert_key(key.to_vec()),
            },
            None => match maybe_prev_key {
                Some(prev_key) => query.insert_key(prev_key.to_vec()),
                None => query.insert_key(vec![]),
            },
        };

        Ok(maybe_entry)
    }
}

pub trait Prove {
    fn prove(&self, query: Query) -> Result<Vec<u8>>;
}

impl Prove for MerkStore {
    fn prove(&self, query: Query) -> Result<Vec<u8>> {
        Ok(self.merk().prove(query)?)
    }
}

impl Prove for Snapshot {
    fn prove(&self, query: Query) -> Result<Vec<u8>> {
        Ok(self.checkpoint.read().unwrap().prove(query)?)
    }
}

impl Prove for MemSnapshot {
    fn prove(&self, query: Query) -> Result<Vec<u8>> {
        Ok(self.use_snapshot(|ss| ss.prove(query))?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::MerkStore;
    use super::*;
    use crate::store::*;
    use merk::proofs::query::verify;
    use tempfile::TempDir;

    fn temp_merk_store() -> MerkStore {
        let temp_dir = TempDir::new().unwrap();
        MerkStore::new(temp_dir.path())
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

        let (proof, _) = builder.build().unwrap();
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

        let (proof, _) = builder.build().unwrap();
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

        let (proof, _) = builder.build().unwrap();
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

        let (proof, _) = builder.build().unwrap();
        let root_hash = store.borrow().merk().root_hash();
        let map = verify(proof.as_slice(), root_hash).unwrap();
        assert!(map.get(&[3, 4, 5]).unwrap().is_some());
        let mut iter = map.range(&[3, 4, 5][..]..=&[3, 4, 7][..]);

        let _res = iter.next().unwrap().unwrap();
        //assert!(res.is_none());
    }
}
