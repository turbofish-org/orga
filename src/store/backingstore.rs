#[cfg(feature = "merk-full")]
use crate::merk::snapshot::Snapshot;
#[cfg(feature = "merk-verify")]
use crate::merk::ProofStore;
#[cfg(feature = "merk-full")]
use crate::merk::{MerkStore, ProofBuilder};
#[cfg(feature = "merk-verify")]
use crate::store::BufStore;
use crate::store::ReadWrite;
use crate::store::{Empty, MapStore, PartialMapStore, Read, Shared, Write, KV};
use crate::{Error, Result};

#[cfg(feature = "merk-full")]
type WrappedMerkStore = Shared<BufStore<Shared<BufStore<Shared<MerkStore>>>>>;

#[derive(Clone)]
pub enum BackingStore {
    MapStore(Shared<MapStore>),
    PartialMapStore(Shared<PartialMapStore>),
    Null(Empty),
    Other(Shared<Box<dyn ReadWrite>>),

    #[cfg(feature = "merk-full")]
    WrappedMerk(WrappedMerkStore),
    #[cfg(feature = "merk-full")]
    ProofBuilder(ProofBuilder<MerkStore>),
    #[cfg(feature = "merk-full")]
    ProofBuilderSnapshot(ProofBuilder<Snapshot>),
    #[cfg(feature = "merk-full")]
    Merk(Shared<MerkStore>),
    #[cfg(feature = "merk-full")]
    Snapshot(Shared<Snapshot>),
    #[cfg(feature = "merk-verify")]
    ProofMap(Shared<ProofStore>),
}

impl Default for BackingStore {
    fn default() -> Self {
        BackingStore::Null(Empty)
    }
}

impl Read for BackingStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self {
            BackingStore::MapStore(ref store) => store.get(key),
            BackingStore::PartialMapStore(ref store) => store.get(key),
            BackingStore::Null(ref null) => null.get(key),
            BackingStore::Other(ref store) => store.borrow().get(key),

            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref store) => store.get(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(ref builder) => builder.get(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilderSnapshot(ref builder) => builder.get(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Merk(ref store) => store.get(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Snapshot(ref store) => store.get(key),
            #[cfg(feature = "merk-verify")]
            BackingStore::ProofMap(ref map) => map.get(key),
        }
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        match self {
            BackingStore::MapStore(ref store) => store.get_next(key),
            BackingStore::PartialMapStore(ref store) => store.get_next(key),
            BackingStore::Null(ref null) => null.get_next(key),
            BackingStore::Other(ref store) => store.borrow().get_next(key),

            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref store) => store.get_next(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(ref builder) => builder.get_next(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilderSnapshot(ref builder) => builder.get_next(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Merk(ref store) => store.get_next(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Snapshot(ref store) => store.get_next(key),
            #[cfg(feature = "merk-verify")]
            BackingStore::ProofMap(ref map) => map.get_next(key),
        }
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        match self {
            BackingStore::MapStore(ref store) => store.get_prev(key),
            BackingStore::PartialMapStore(ref store) => store.get_prev(key),
            BackingStore::Null(ref null) => null.get_prev(key),
            BackingStore::Other(ref store) => store.borrow().get_prev(key),

            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref store) => store.get_prev(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(ref builder) => builder.get_prev(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilderSnapshot(ref builder) => builder.get_prev(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Merk(ref store) => store.get_prev(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Snapshot(ref store) => store.get_prev(key),
            #[cfg(feature = "merk-verify")]
            BackingStore::ProofMap(ref map) => map.get_prev(key),
        }
    }
}

impl Write for BackingStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        match self {
            BackingStore::MapStore(ref mut store) => store.put(key, value),
            BackingStore::PartialMapStore(_) => {
                panic!("put() is not implemented for PartialMapStore")
            }
            BackingStore::Null(ref mut store) => store.put(key, value),
            BackingStore::Other(ref mut store) => store.borrow_mut().put(key, value),

            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref mut store) => store.put(key, value),
            #[cfg(feature = "merk-full")]
            BackingStore::Merk(ref mut store) => store.put(key, value),
            #[cfg(feature = "merk-full")]
            BackingStore::Snapshot(_) => {
                panic!("put() is not implemented for Snapshot")
            }
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(_) => {
                panic!("put() is not implemented for ProofBuilder")
            }
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilderSnapshot(_) => {
                panic!("put() is not implemented for ProofBuilderSnapshot")
            }
            #[cfg(feature = "merk-verify")]
            BackingStore::ProofMap(_) => {
                panic!("put() is not implemented for ProofMap")
            }
        }
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        match self {
            BackingStore::MapStore(ref mut store) => store.delete(key),
            BackingStore::PartialMapStore(_) => {
                panic!("delete() is not implemented for PartialMapStore")
            }
            BackingStore::Null(ref mut store) => store.delete(key),
            BackingStore::Other(ref mut store) => store.borrow_mut().delete(key),

            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(ref mut store) => store.delete(key),
            #[cfg(feature = "merk-full")]
            BackingStore::Merk(ref mut store) => store.delete(key),
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(_) => {
                panic!("delete() is not implemented for ProofBuilder")
            }
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilderSnapshot(_) => {
                panic!("delete() is not implemented for ProofBuilderSnapshot")
            }
            #[cfg(feature = "merk-full")]
            BackingStore::Snapshot(_) => {
                panic!("delete() is not implemented for Snapshot")
            }
            #[cfg(feature = "merk-verify")]
            BackingStore::ProofMap(_) => {
                panic!("delete() is not implemented for ProofMap")
            }
        }
    }
}

impl BackingStore {
    #[cfg(feature = "merk-full")]
    pub fn into_proof_builder(self) -> Result<ProofBuilder<MerkStore>> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilder(builder) => Ok(builder),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to proof builder".into(),
            )),
        }
    }

    #[cfg(feature = "merk-full")]
    pub fn into_proof_builder_snapshot(self) -> Result<ProofBuilder<Snapshot>> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::ProofBuilderSnapshot(builder) => Ok(builder),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to proof builder snapshot".into(),
            )),
        }
    }

    #[cfg(feature = "merk-full")]
    pub fn into_wrapped_merk(self) -> Result<WrappedMerkStore> {
        match self {
            #[cfg(feature = "merk-full")]
            BackingStore::WrappedMerk(store) => Ok(store),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to wrapped merk".into(),
            )),
        }
    }

    pub fn into_map_store(self) -> Result<Shared<MapStore>> {
        match self {
            BackingStore::MapStore(store) => Ok(store),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to map store".into(),
            )),
        }
    }

    pub fn into_other(self) -> Result<Shared<Box<dyn ReadWrite>>> {
        match self {
            BackingStore::Other(store) => Ok(store),
            _ => Err(Error::Downcast(
                "Failed to downcast backing store to map store".into(),
            )),
        }
    }

    #[cfg(feature = "merk-full")]
    pub fn use_merk<F: FnOnce(&merk::Merk) -> T, T>(&self, f: F) -> T {
        match self {
            BackingStore::WrappedMerk(_store) => todo!(),
            BackingStore::Merk(store) => {
                let borrow = store.borrow();
                f(borrow.merk())
            }
            BackingStore::Snapshot(store) => {
                let borrow = store.borrow();
                let borrow = borrow.checkpoint.borrow();
                f(&borrow)
            }
            _ => panic!("Cannot get MerkStore from BackingStore variant"),
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
