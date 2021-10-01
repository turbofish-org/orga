use super::{MerkStore, ProofBuilder};
use merk::proofs::query::Map as ProofMap;
use crate::store::{BufStore, MapStore, Read, Shared, Write, KV};
use crate::Result;
use failure::bail;
use std::ops::Bound;

type WrappedMerkStore = Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>;
#[derive(Clone)]
pub enum BackingStore {
    WrappedMerk(WrappedMerkStore),
    ProofBuilder(ProofBuilder),
    MapStore(Shared<MapStore>),
    ProofMap(Shared<ProofStore>),
}

impl Read for BackingStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self {
            BackingStore::WrappedMerk(ref store) => store.get(key),
            BackingStore::ProofBuilder(ref builder) => builder.get(key),
            BackingStore::MapStore(ref store) => store.get(key),
            BackingStore::ProofMap(ref map) => map.get(key),
        }
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        match self {
            BackingStore::WrappedMerk(ref store) => store.get_next(key),
            BackingStore::ProofBuilder(ref builder) => builder.get_next(key),
            BackingStore::MapStore(ref store) => store.get_next(key),
            BackingStore::ProofMap(ref map) => map.get_next(key),
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
            BackingStore::MapStore(ref mut store) => store.put(key, value),
            BackingStore::ProofMap(_) => {
                panic!("put() is not implemented for ProofMap")
            }
        }
    }
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        match self {
            BackingStore::WrappedMerk(ref mut store) => store.delete(key),
            BackingStore::ProofBuilder(_) => {
                panic!("delete() is not implemented for ProofBuilder")
            }
            BackingStore::MapStore(ref mut store) => store.delete(key),
            BackingStore::ProofMap(_) => {
                panic!("delete() is not implemented for ProofMap")
            }
        }
    }
}

impl BackingStore {
    pub fn into_proof_builder(self) -> Result<ProofBuilder> {
        match self {
            BackingStore::ProofBuilder(builder) => Ok(builder),
            _ => bail!("Failed to downcast backing store to proof builder"),
        }
    }

    pub fn into_wrapped_merk(self) -> Result<WrappedMerkStore> {
        match self {
            BackingStore::WrappedMerk(store) => Ok(store),
            _ => bail!("Failed to downcast backing store to wrapped merk"),
        }
    }

    pub fn into_map_store(self) -> Result<Shared<MapStore>> {
        match self {
            BackingStore::MapStore(store) => Ok(store),
            _ => bail!("Failed to downcast backing store to map store"),
        }
    }

    pub fn into_proof_map(self) -> Result<Shared<ProofStore>> {
        match self {
            BackingStore::ProofMap(map) => Ok(map),
            _ => bail!("Failed to downcast backing store to proof map"),
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

impl From<Shared<MapStore>> for BackingStore {
    fn from(store: Shared<MapStore>) -> BackingStore {
        BackingStore::MapStore(store)
    }
}

impl From<Shared<ProofStore>> for BackingStore {
    fn from(store: Shared<ProofStore>) -> BackingStore {
        BackingStore::ProofMap(store)
    }
}

pub struct ProofStore(pub ProofMap);

impl Read for ProofStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let maybe_value = self.0.get(key)?;
        Ok(maybe_value.map(|value| value.to_vec()))
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let mut iter = self.0.range((Bound::Excluded(key), Bound::Unbounded));
        let item = iter.next().transpose()?;
        Ok(item.map(|(k, v)| (k.to_vec(), v.to_vec())))
    }
}

