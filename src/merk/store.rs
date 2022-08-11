#[cfg(test)]
use mutagen::mutate;

use crate::abci::ABCIStore;
use crate::error::{Error, Result};
use crate::store::*;
use merk::{
    chunks::ChunkProducer, restore::Restorer, rocksdb, tree::Tree, BatchEntry, Hash, Merk, Op,
};
use std::cell::RefCell;
use std::{collections::BTreeMap, convert::TryInto};
use std::{
    mem::transmute,
    path::{Path, PathBuf},
};
use tendermint_proto::abci::{self, *};
type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

pub const SNAPSHOT_INTERVAL: u64 = 1000;
pub const SNAPSHOT_LIMIT: u64 = 4;
pub const FIRST_SNAPSHOT_HEIGHT: u64 = 2;

struct MerkSnapshot {
    _checkpoint: Merk,
    chunks: RefCell<Option<ChunkProducer<'static>>>,
    length: u32,
    hash: Hash,
}

impl MerkSnapshot {
    fn new(checkpoint: Merk) -> Result<Self> {
        let chunks = checkpoint.chunks()?;
        let chunks: ChunkProducer<'static> = unsafe { transmute(chunks) };

        let length = chunks.len() as u32;
        let hash = checkpoint.root_hash();

        Ok(Self {
            _checkpoint: checkpoint,
            chunks: RefCell::new(Some(chunks)),
            length,
            hash,
        })
    }

    fn chunk(&self, index: usize) -> Result<Vec<u8>> {
        let mut chunks = self.chunks.borrow_mut();
        let chunks = chunks.as_mut().unwrap();
        let chunk = chunks.chunk(index)?;
        Ok(chunk)
    }
}

impl Drop for MerkSnapshot {
    fn drop(&mut self) {
        // drop the self-referential ChunkProducer before the Merk instance
        self.chunks.borrow_mut().take();
    }
}

/// A [`store::Store`] implementation backed by a [`merk`](https://docs.rs/merk)
/// Merkle key/value store.
pub struct MerkStore {
    merk: Option<Merk>,
    home: PathBuf,
    map: Option<Map>,
    snapshots: BTreeMap<u64, MerkSnapshot>,
    restorer: Option<Restorer>,
    target_snapshot: Option<Snapshot>,
}

impl MerkStore {
    /// Constructs a `MerkStore` which references the given
    /// [`Merk`](https://docs.rs/merk/latest/merk/struct.Merk.html) inside the
    /// `merk_home` directory. Initializes a new Merk instance if the directory
    /// is empty
    #[cfg_attr(test, mutate)]
    pub fn new(home: PathBuf) -> Self {
        let merk = Merk::open(&home.join("db")).unwrap();

        // TODO: return result instead of panicking
        maybe_remove_restore(&home).expect("Failed to remove incomplete state sync restore");

        let snapshot_path = home.join("snapshots");

        let no_snapshot = !snapshot_path.exists();
        if no_snapshot {
            std::fs::create_dir(&snapshot_path).expect("Failed to create 'snapshots' directory");
        }

        let snapshots = load_snapshots(&home).expect("Failed to load snapshots");

        MerkStore {
            map: Some(Default::default()),
            merk: Some(merk),
            home,
            snapshots,
            target_snapshot: None,
            restorer: None,
        }
    }

    fn path<T: ToString>(&self, name: T) -> PathBuf {
        self.home.join(name.to_string())
    }

    /// Flushes writes to the underlying `Merk` store.
    ///
    /// `aux` may contain auxilary keys and values to be written to the
    /// underlying store, which will not affect the Merkle tree but will still
    /// be persisted in the database.
    pub(super) fn write(&mut self, aux: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Result<()> {
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

    pub(crate) fn merk(&self) -> &Merk {
        self.merk.as_ref().unwrap()
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
        // TODO: use an iterator in merk which steps through in-memory nodes
        // (loading if necessary)
        let mut iter = self.merk().raw_iter();
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
}

pub struct Iter<'a> {
    iter: rocksdb::DBRawIterator<'a>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.iter.valid() {
            return None;
        }

        // here we use unsafe code to add lifetimes, since rust-rocksdb just
        // returns the data with no lifetimes. the transmute calls convert from
        // `&[u8]` to `&'a [u8]`, so there is no way this can make things *less*
        // correct.
        let entry = unsafe {
            (
                transmute(self.iter.key().unwrap()),
                transmute(self.iter.value().unwrap()),
            )
        };
        self.iter.next();
        Some(entry)
    }
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
        use sha2::{Digest, Sha512_256};

        let mut hasher = Sha512_256::new();
        hasher.update(b"ibc");
        hasher.update(merk_root);

        Ok(hasher.finalize().to_vec())
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        let height_bytes = height.to_be_bytes();

        let metadata = vec![(b"height".to_vec(), Some(height_bytes.to_vec()))];

        self.write(metadata)?;
        self.merk.as_mut().unwrap().flush()?;

        #[cfg(feature = "state-sync")]
        self.maybe_create_snapshot()?;

        Ok(())
    }

    fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        let snapshots = self
            .snapshots
            .iter()
            .map(|(height, snapshot)| Snapshot {
                chunks: snapshot.length,
                hash: snapshot.hash.to_vec(),
                height: *height,
                ..Default::default()
            })
            .collect();
        Ok(snapshots)
    }

    fn load_snapshot_chunk(&self, req: RequestLoadSnapshotChunk) -> Result<Vec<u8>> {
        match self.snapshots.get(&req.height) {
            Some(snapshot) => snapshot.chunk(req.chunk as usize),
            // ABCI has no way to specify that we don't have the requested
            // chunk, so we just return an empty one (and probably get banned by
            // the client when they try to verify)
            None => Ok(vec![]),
        }
    }

    fn apply_snapshot_chunk(&mut self, req: RequestApplySnapshotChunk) -> Result<()> {
        let restore_path = self.home.join("restore");
        let target_snapshot = self
            .target_snapshot
            .as_mut()
            .expect("Tried to apply a snapshot chunk while no state sync is in progress");

        if self.restorer.is_none() {
            let expected_hash: [u8; 32] = match target_snapshot.hash.clone().try_into() {
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
        let chunks_remaining = restorer.process_chunk(req.chunk.as_slice())?;
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
            if is_canonical_height && snapshot.hash == req.app_hash {
                self.target_snapshot = Some(snapshot);
                res.set_result(abci::response_offer_snapshot::Result::Accept);
            }
        }

        Ok(res)
    }
}

impl MerkStore {
    fn maybe_create_snapshot(&mut self) -> Result<()> {
        let height = self.height()?;
        if height == 0 || (height % SNAPSHOT_INTERVAL != 0 && height != FIRST_SNAPSHOT_HEIGHT) {
            return Ok(());
        }
        if self.snapshots.contains_key(&height) {
            return Ok(());
        }

        let path = self.snapshot_path(height);
        let merk = self.merk.as_ref().unwrap();
        let checkpoint = merk.checkpoint(path)?;

        let snapshot = MerkSnapshot::new(checkpoint)?;
        self.snapshots.insert(height, snapshot);

        self.maybe_prune_snapshots()
    }

    fn maybe_prune_snapshots(&mut self) -> Result<()> {
        let height = self.height()?;
        if height <= SNAPSHOT_INTERVAL * SNAPSHOT_LIMIT {
            return Ok(());
        }

        // TODO: iterate through snapshot map rather than just pruning the
        // expected oldest one

        let remove_height = height - SNAPSHOT_INTERVAL * SNAPSHOT_LIMIT;
        self.snapshots.remove(&remove_height);

        let path = self.snapshot_path(remove_height);
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }

        Ok(())
    }

    fn snapshot_path(&self, height: u64) -> PathBuf {
        self.path("snapshots").join(height.to_string())
    }
}

fn maybe_remove_restore(home: &Path) -> Result<()> {
    let restore_path = home.join("restore");
    if restore_path.exists() {
        std::fs::remove_dir_all(&restore_path)?;
    }

    Ok(())
}

fn load_snapshots(home: &Path) -> Result<BTreeMap<u64, MerkSnapshot>> {
    let mut snapshots = BTreeMap::new();

    let snapshot_dir = home.join("snapshots").read_dir()?;
    for entry in snapshot_dir {
        let entry = entry?;
        let path = entry.path();

        // TODO: open read-only
        let checkpoint = Merk::open(&path)?;
        let snapshot = MerkSnapshot::new(checkpoint)?;

        let height_str = path.file_name().unwrap().to_str().unwrap();
        let height: u64 = height_str.parse()?;
        snapshots.insert(height, snapshot);
    }

    Ok(snapshots)
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut array = [0; 8];
    array.copy_from_slice(bytes);
    u64::from_be_bytes(array)
}
