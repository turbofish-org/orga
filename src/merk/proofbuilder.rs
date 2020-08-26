use std::cell::Cell;
use std::collections::BTreeSet;

use crate::Result;
use crate::store::Read;
use super::MerkStore;

pub struct ProofBuilder<'a> {
    store: &'a MerkStore<'a>,
    keys: Cell<BTreeSet<Vec<u8>>>
}

impl<'a> ProofBuilder<'a> {
    pub fn new(store: &'a MerkStore<'a>) -> Self {
        ProofBuilder {
            store,
            keys: Cell::new(BTreeSet::new())
        }
    }

    pub fn build(self) -> Result<Vec<u8>> {
        let keys = self.keys.take();
        let keys: Vec<Vec<u8>> = keys.into_iter().collect();
        self.store.merk().prove(keys.as_slice())
    }
}

impl<'a> Read for ProofBuilder<'a> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut keys = self.keys.take();
        keys.insert(key.to_vec());
        self.keys.set(keys);

        self.store.get(key)
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
        builder.get(&key[..]).unwrap();
    
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
        builder.get(&key[..]).unwrap();
    
        let proof = builder.build().unwrap();
        let root_hash = merk.root_hash();
        let res = verify_proof(proof.as_slice(), &[vec![5]], root_hash).unwrap();

        assert_eq!(res[0], None);
    }
}
