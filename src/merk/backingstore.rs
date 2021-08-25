use super::{MerkStore, ProofBuilder};
use crate::store::{BufStore, Read, Shared, Write, KV};
use crate::Result;

type WrappedMerkStore = Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>;
pub enum BackingStore<'a> {
    WrappedMerk(WrappedMerkStore),
    ProofBuilder(ProofBuilder<'a>),
}

impl<'a> Read for BackingStore<'a> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self {
            BackingStore::WrappedMerk(ref store) => store.get(key),
            BackingStore::ProofBuilder(ref builder) => builder.get(key),
        }
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        match self {
            BackingStore::WrappedMerk(ref store) => store.get_next(key),
            BackingStore::ProofBuilder(ref builder) => builder.get_next(key),
        }
    }
}

impl<'a> Write for BackingStore<'a> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        match self {
            BackingStore::WrappedMerk(ref mut store) => store.put(key, value),
            BackingStore::ProofBuilder(_) => {
                panic!("put() is not implemented for ProofBuilder")
            }
        }
    }
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        match self {
            BackingStore::WrappedMerk(ref mut store) => store.delete(key),
            BackingStore::ProofBuilder(_) => {
                panic!("delete() is not implemented for ProofBuilder")
            }
        }
    }
}

impl<'a> From<WrappedMerkStore> for BackingStore<'a> {
    fn from(store: WrappedMerkStore) -> BackingStore<'a> {
        BackingStore::WrappedMerk(store)
    }
}
