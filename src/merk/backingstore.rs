use super::MerkStore;
use crate::store::{BufStore, Read, Shared, Write, KV};
use crate::Result;

type WrappedMerkStore = Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>;
pub enum BackingStore {
    WrappedMerk(WrappedMerkStore),
    ProofBuilder,
}

impl Read for BackingStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self {
            BackingStore::WrappedMerk(ref store) => store.get(key),
            _ => todo!(),
        }
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        match self {
            BackingStore::WrappedMerk(ref store) => store.get_next(key),
            _ => todo!(),
        }
    }
}

impl Write for BackingStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        match self {
            BackingStore::WrappedMerk(ref mut store) => store.put(key, value),
            _ => todo!(),
        }
    }
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        match self {
            BackingStore::WrappedMerk(ref mut store) => store.delete(key),
            _ => todo!(),
        }
    }
}

impl From<WrappedMerkStore> for BackingStore {
    fn from(store: WrappedMerkStore) -> BackingStore {
        BackingStore::WrappedMerk(store)
    }
}
