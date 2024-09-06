//! A read-only store backed by a snapshot.
use crate::{
    store::{Read, Shared},
    Result,
};

use super::MerkStore;

/// A KV store backed by a [merk::snapshot::StaticSnapshot].
///
/// This is used to provide a read-only view into the Merk tree at a given point
/// in time.
pub struct MemSnapshot {
    snapshot: merk::snapshot::StaticSnapshot,
    merk_store: Shared<MerkStore>,
}

impl MemSnapshot {
    /// Create a new [MemSnapshot] from a [merk::snapshot::StaticSnapshot].
    pub fn new(
        mut snapshot: merk::snapshot::StaticSnapshot,
        merk_store: Shared<MerkStore>,
    ) -> Self {
        snapshot.should_drop = true;
        Self {
            snapshot,
            merk_store,
        }
    }

    /// Use the snapshot to run the provided function.
    pub fn use_snapshot<R, F: FnOnce(&merk::Snapshot) -> R>(&self, f: F) -> R {
        let store = self.merk_store.borrow();
        let db = store.merk().db();
        let ss = unsafe { self.snapshot.with_db(db) };
        f(&ss)
    }
}

impl Read for MemSnapshot {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.use_snapshot(|ss| ss.get(key))?)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<crate::store::KV>> {
        self.use_snapshot(|ss| {
            let iter = ss.raw_iter();
            super::store::get_next(iter, key)
        })
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<crate::store::KV>> {
        self.use_snapshot(|ss| {
            let iter = ss.raw_iter();
            super::store::get_prev(iter, key)
        })
    }
}
