use super::{MerkStore, ProofBuilder};
use crate::store::{BufStore, Read, Shared, Write, KV};
use crate::Result;
use failure::bail;

type WrappedMerkStore = Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>;
#[derive(Clone)]
pub enum BackingStore {
    WrappedMerk(WrappedMerkStore),
    ProofBuilder(ProofBuilder),
}

impl Read for BackingStore {
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

impl Write for BackingStore {
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

impl BackingStore {
    pub fn as_proof_builder(self) -> Result<ProofBuilder> {
        match self {
            BackingStore::ProofBuilder(builder) => Ok(builder),
            _ => bail!("Failed to downcast backing store to proof builder"),
        }
    }

    pub fn as_wrapped_merk(self) -> Result<WrappedMerkStore> {
        match self {
            BackingStore::WrappedMerk(store) => Ok(store),
            _ => bail!("Failed to downcast backing store to wrapped merk"),
        }
    }
}

impl From<WrappedMerkStore> for BackingStore {
    fn from(store: WrappedMerkStore) -> BackingStore {
        BackingStore::WrappedMerk(store)
    }
}

impl From<Shared<MerkStore>> for BackingStore {
    fn from(store: Shared<MerkStore>) -> BackingStore {
        let builder = ProofBuilder::new(store);
        BackingStore::ProofBuilder(builder)
    }
}
