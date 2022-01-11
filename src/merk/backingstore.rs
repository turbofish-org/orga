#[cfg(test)]
use mutagen::mutate;

#[cfg(feature = "merk-full")]
use super::{MerkStore, ProofBuilder};
use crate::store::{BufStore, MapStore, Read, Shared, Write, KV};
use crate::{Error, Result};
use merk::proofs::query::Map as ProofMap;
use std::ops::Bound;

#[cfg(feature = "merk-full")]
type WrappedMerkStore = Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>;

#[derive(Clone)]
pub enum BackingStore {
    #[cfg(feature = "merk-full")]
    WrappedMerk(WrappedMerkStore),
    #[cfg(feature = "merk-full")]
    ProofBuilder(ProofBuilder),
    MapStore(Shared<MapStore>),
    ProofMap(Shared<ABCIPrefixedProofStore>),
}

impl Read for BackingStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref store) => store.get(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(ref builder) => builder.get(key),
            BackingStore::MapStore(ref store) => store.get(key),
            BackingStore::ProofMap(ref map) => map.get(key),
        }
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref store) => store.get_next(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(ref builder) => builder.get_next(key),
            BackingStore::MapStore(ref store) => store.get_next(key),
            BackingStore::ProofMap(ref map) => map.get_next(key),
        }
    }
}

impl Write for BackingStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref mut store) => store.put(key, value),
            #[cfg(feature = "merk-full")]
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
            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref mut store) => store.delete(key),
            #[cfg(feature = "merk-full")]
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
    #[cfg(feature = "merk-full")]    
    #[cfg_attr(test, mutate)]
    pub fn into_proof_builder(self) -> Result<ProofBuilder> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(builder) => Ok(builder),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to proof builder".into(),
            )),
        }
    }

    #[cfg(feature = "merk-full")]
    #[cfg_attr(test, mutate)]
    pub fn into_wrapped_merk(self) -> Result<WrappedMerkStore> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(store) => Ok(store),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to wrapped merk".into(),
            )),
        }
    }

    #[cfg_attr(test, mutate)]
    pub fn into_map_store(self) -> Result<Shared<MapStore>> {
        match self {
            BackingStore::MapStore(store) => Ok(store),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to map store".into(),
            )),
        }
    }

    #[cfg_attr(test, mutate)]
    pub fn into_abci_prefixed_proof_map(self) -> Result<Shared<ABCIPrefixedProofStore>> {
        match self {
            BackingStore::ProofMap(store) => Ok(store),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to ABCI-prefixed proof map".into(),
            )),
        }
    }
}

#[cfg(feature = "merk-full")]
impl From<WrappedMerkStore> for BackingStore {
    fn from(store: WrappedMerkStore) -> BackingStore {
        BackingStore::WrappedMerk(store)
    }
}

#[cfg(feature = "merk-full")]
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

impl From<Shared<ABCIPrefixedProofStore>> for BackingStore {
    fn from(store: Shared<ABCIPrefixedProofStore>) -> BackingStore {
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

pub struct ABCIPrefixedProofStore(pub ProofStore);

impl ABCIPrefixedProofStore {
    #[cfg_attr(test, mutate)]
    pub fn new(map: ProofMap) -> Self {
        ABCIPrefixedProofStore(ProofStore(map))
    }

    fn prefix_key(key: &[u8]) -> Vec<u8> {
        let mut prefixed_key = Vec::with_capacity(key.len() + 1);
        prefixed_key.push(0);
        prefixed_key.extend_from_slice(key);
        prefixed_key
    }

    fn deprefix_key(mut key: Vec<u8>) -> Vec<u8> {
        key.remove(0);
        key
    }
}

impl Read for ABCIPrefixedProofStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let key = Self::prefix_key(key);
        self.0.get(key.as_slice())
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let key = Self::prefix_key(key);
        let maybe_kv = self
            .0
            .get_next(key.as_slice())?
            .map(|(key, value)| (Self::deprefix_key(key), value));
        Ok(maybe_kv)
    }
}
