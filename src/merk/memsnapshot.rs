use crate::{
    store::{Read, Shared, Write},
    Result,
};

use super::MerkStore;

pub struct MemSnapshot {
    pub(crate) snapshot: merk::snapshot::StaticSnapshot,
    pub(crate) merk_store: Shared<MerkStore>,
}

impl MemSnapshot {
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
