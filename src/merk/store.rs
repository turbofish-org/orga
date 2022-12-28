use crate::abci::ABCIStore;
use crate::error::{Error, Result};
use crate::store::*;
use merk::{
    chunks::ChunkProducer, proofs::query::Map as ProofMap, restore::Restorer, rocksdb, tree::Tree,
    BatchEntry, Hash, Merk, Op,
};
use std::cell::RefCell;
use std::{collections::BTreeMap, convert::TryInto};
use std::{
    mem::transmute,
    ops::Bound,
    path::{Path, PathBuf},
};
use tendermint_proto::v0_34::abci::{self, *};

use super::snapshot;
type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

pub const SNAPSHOT_INTERVAL: u64 = 1000;
pub const FIRST_SNAPSHOT_HEIGHT: u64 = 2;

/// A [`store::Store`] implementation backed by a [`merk`](https://docs.rs/merk)
/// Merkle key/value store.
pub struct MerkStore {
    merk: Option<Merk>,
    home: PathBuf,
    map: Option<Map>,
    snapshots: snapshot::Snapshots,
    restorer: Option<Restorer>,
    target_snapshot: Option<Snapshot>,
}

impl MerkStore {
    /// Constructs a `MerkStore` which references the given
    /// [`Merk`](https://docs.rs/merk/latest/merk/struct.Merk.html) inside the
    /// `merk_home` directory. Initializes a new Merk instance if the directory
    /// is empty
    pub fn new<P: AsRef<Path>>(home: P) -> Self {
        let home = home.as_ref().to_path_buf();
        let merk = Merk::open(home.join("db")).unwrap();

        // TODO: return result instead of panicking
        maybe_remove_restore(&home).expect("Failed to remove incomplete state sync restore");

        let snapshot_path = home.join("snapshots");
        let snapshots = snapshot::Snapshots::load(snapshot_path.as_path())
            .expect("Failed to load snapshots")
            // TODO: make configurable
            .with_filters(vec![
                #[cfg(feature = "state-sync")]
                snapshot::SnapshotFilter::specific_height(2, None),
                #[cfg(feature = "state-sync")]
                snapshot::SnapshotFilter::interval(1000, 4),
                snapshot::SnapshotFilter::interval(1, 20),
            ]);

        MerkStore {
            map: Some(Default::default()),
            merk: Some(merk),
            home,
            snapshots,
            target_snapshot: None,
            restorer: None,
        }
    }

    pub fn init_from(
        source: impl AsRef<Path>,
        dest: impl AsRef<Path>,
        height: Option<u64>,
    ) -> Result<Self> {
        // TODO: error if source isn't already a merk (currently creates a new merk when opening with `new`)
        let source = source.as_ref();
        let dest = dest.as_ref();

        let source = Merk::open(source.join("db"))?;
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        source.checkpoint(dest.join("db"))?;
        let mut merk_store = Self::new(dest);

        if let Some(height) = height {
            merk_store
                .write(vec![(
                    b"height".to_vec(),
                    Some(height.to_be_bytes().to_vec()),
                )])
                .unwrap();
        }

        Ok(merk_store)
    }

    fn path<T: ToString>(&self, name: T) -> PathBuf {
        self.home.join(name.to_string())
    }

    /// Flushes writes to the underlying `Merk` store.
    ///
    /// `aux` may contain auxilary keys and values to be written to the
    /// underlying store, which will not affect the Merkle tree but will still
    /// be persisted in the database.
    pub fn write(&mut self, aux: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Result<()> {
        let map = self.map.take().unwrap();
        self.map = Some(Map::new());

        let batch = to_batch(map);
        let aux_batch = to_batch(aux);

        Ok(self
            .merk
            .as_mut()
            .unwrap()
            .apply(batch.as_ref(), aux_batch.as_ref())?)
    }

    pub fn merk(&self) -> &Merk {
        self.merk.as_ref().unwrap()
    }

    pub(crate) fn snapshots(&self) -> &snapshot::Snapshots {
        &self.snapshots
    }
}

/// Collects an iterator of key/value entries into a `Vec`.
fn to_batch<I: IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>)>>(i: I) -> Vec<BatchEntry> {
    let mut batch = Vec::new();
    for (key, val) in i {
        match val {
            Some(val) => batch.push((key, Op::Put(val))),
            None => batch.push((key, Op::Delete)),
        }
    }
    batch
}

impl Read for MerkStore {
    /// Gets a value from the underlying `Merk` store.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.as_ref().unwrap().get(key) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => Ok(self.merk.as_ref().unwrap().get(key)?),
        }
    }

    fn get_next(&self, start: &[u8]) -> Result<Option<KV>> {
        get_next(self.merk(), start)
    }

    fn get_prev(&self, end: Option<&[u8]>) -> Result<Option<KV>> {
        // TODO: use an iterator in merk which steps through in-memory nodes
        // (loading if necessary)
        let mut iter = self.merk().raw_iter();
        if let Some(key) = end {
            iter.seek(key);

            if !iter.valid() {
                iter.status()?;
                return Ok(None);
            }

            if iter.key().unwrap() == key {
                iter.prev();

                if !iter.valid() {
                    iter.status()?;
                    return Ok(None);
                }
            }
        } else {
            iter.seek_to_last();

            if !iter.valid() {
                iter.status()?;
                return Ok(None);
            }
        }

        let key = iter.key().unwrap();
        let tree_bytes = iter.value().unwrap();
        let tree = Tree::decode(vec![], tree_bytes);
        let value = tree.value();
        Ok(Some((key.to_vec(), value.to_vec())))
    }
}

pub(crate) fn get_next(merk: &Merk, start: &[u8]) -> Result<Option<KV>> {
    // TODO: use an iterator in merk which steps through in-memory nodes
    // (loading if necessary)
    let mut iter = merk.raw_iter();
    iter.seek(start);

    if !iter.valid() {
        iter.status()?;
        return Ok(None);
    }

    if iter.key().unwrap() == start {
        iter.next();

        if !iter.valid() {
            iter.status()?;
            return Ok(None);
        }
    }

    let key = iter.key().unwrap();
    let tree_bytes = iter.value().unwrap();
    let tree = Tree::decode(vec![], tree_bytes);
    let value = tree.value();
    Ok(Some((key.to_vec(), value.to_vec())))
}

impl Write for MerkStore {
    /// Writes a value to the underlying `Merk` store.
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.as_mut().unwrap().insert(key, Some(value));
        Ok(())
    }

    /// Deletes a value from the underlying `Merk` store.
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.map.as_mut().unwrap().insert(key.to_vec(), None);
        Ok(())
    }
}

fn calc_app_hash(merk_root: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha512_256};

    let mut hasher = Sha512_256::new();
    hasher.update(b"ibc");
    hasher.update(merk_root);

    hasher.finalize().to_vec()
}

impl ABCIStore for MerkStore {
    fn height(&self) -> Result<u64> {
        let maybe_bytes = self.merk().get_aux(b"height")?;
        match maybe_bytes {
            None => Ok(0),
            Some(bytes) => Ok(read_u64(&bytes)),
        }
    }

    fn root_hash(&self) -> Result<Vec<u8>> {
        let merk_root = self.merk.as_ref().unwrap().root_hash();

        Ok(calc_app_hash(merk_root.as_slice()))
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        let height_bytes = height.to_be_bytes();

        let metadata = vec![(b"height".to_vec(), Some(height_bytes.to_vec()))];

        self.write(metadata)?;
        self.merk.as_mut().unwrap().flush()?;

        #[cfg(feature = "state-sync")]
        if self.snapshots.should_create(height) {
            let path = self.snapshots.path(height);
            let checkpoint = self.merk.as_ref().unwrap().checkpoint(path)?;
            self.snapshots.create(height, checkpoint)?;
        }

        Ok(())
    }

    fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        let snapshots = self
            .snapshots
            .iter()
            .map(|(height, snapshot)| Snapshot {
                chunks: snapshot.length,
                hash: snapshot.hash.to_vec().into(),
                height: *height,
                ..Default::default()
            })
            .collect();
        Ok(snapshots)
    }

    fn load_snapshot_chunk(&self, req: RequestLoadSnapshotChunk) -> Result<Vec<u8>> {
        self.snapshots.abci_load_chunk(req)
    }

    fn apply_snapshot_chunk(&mut self, req: RequestApplySnapshotChunk) -> Result<()> {
        let restore_path = self.home.join("restore");
        let target_snapshot = self
            .target_snapshot
            .as_mut()
            .expect("Tried to apply a snapshot chunk while no state sync is in progress");

        if self.restorer.is_none() {
            let expected_hash: [u8; 32] = match target_snapshot.hash.to_vec().try_into() {
                Ok(inner) => inner,
                Err(_) => {
                    return Err(Error::Store("Failed to convert expected root hash".into()));
                }
            };

            let restorer = Restorer::new(
                &restore_path,
                expected_hash,
                target_snapshot.chunks as usize,
            )?;
            self.restorer = Some(restorer);
        }

        let restorer = self.restorer.as_mut().unwrap();
        let chunks_remaining = restorer.process_chunk(req.chunk.to_vec().as_slice())?;
        if chunks_remaining == 0 {
            let restored = self.restorer.take().unwrap().finalize()?;
            self.merk.take().unwrap().destroy()?;
            let db_path = self.path("db");
            drop(restored);

            std::fs::rename(&restore_path, &db_path)?;
            self.merk = Some(Merk::open(db_path)?);

            // TODO: write height and flush before renaming db for atomicity
            let height = self.target_snapshot.as_ref().unwrap().height;
            let height_bytes = height.to_be_bytes().to_vec();
            let metadata = vec![(b"height".to_vec(), Some(height_bytes))];
            self.write(metadata)?;
            self.merk.as_mut().unwrap().flush()?;
        }

        Ok(())
    }

    fn offer_snapshot(&mut self, req: RequestOfferSnapshot) -> Result<ResponseOfferSnapshot> {
        let mut res = ResponseOfferSnapshot::default();
        res.set_result(abci::response_offer_snapshot::Result::Reject);

        if let Some(snapshot) = req.snapshot {
            let is_canonical_height = snapshot.height % SNAPSHOT_INTERVAL == 0
                || snapshot.height == FIRST_SNAPSHOT_HEIGHT;
            if is_canonical_height
                && calc_app_hash(snapshot.hash.to_vec().as_slice()) == req.app_hash
            {
                self.target_snapshot = Some(snapshot);
                res.set_result(abci::response_offer_snapshot::Result::Accept);
            }
        }

        Ok(res)
    }
}

fn maybe_remove_restore(home: &Path) -> Result<()> {
    let restore_path = home.join("restore");
    if restore_path.exists() {
        std::fs::remove_dir_all(&restore_path)?;
    }

    Ok(())
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut array = [0; 8];
    array.copy_from_slice(bytes);
    u64::from_be_bytes(array)
}

pub struct ProofStore(pub ProofMap);

impl Read for ProofStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let maybe_value = self.0.get(key).map_err(|err| {
            if let merk::Error::MissingData = err {
                Error::StoreErr(crate::store::Error::ReadUnknown(key.to_vec()))
            } else {
                Error::Merk(err)
            }
        })?;
        Ok(maybe_value.map(|value| value.to_vec()))
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let mut iter = self.0.range((Bound::Excluded(key), Bound::Unbounded));
        let item = iter.next().transpose().map_err(|err| {
            if let merk::Error::MissingData = err {
                Error::StoreErr(crate::store::Error::ReadUnknown(key.to_vec()))
            } else {
                Error::Merk(err)
            }
        })?;
        Ok(item.map(|(k, v)| (k.to_vec(), v.to_vec())))
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        let mut iter = self.0.range((
            Bound::Unbounded,
            key.map_or(Bound::Unbounded, Bound::Excluded),
        ));
        let item = iter.next_back().transpose()?;
        Ok(item.map(|(k, v)| (k.to_vec(), v.to_vec())))
    }
}
