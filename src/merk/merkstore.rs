use crate::abci::ABCIStore;
use crate::error::Result;
use crate::store::*;
use failure::format_err;
use merk::{restore::Restorer, rocksdb, tree::Tree, BatchEntry, Merk, Op};
use std::{collections::BTreeMap, convert::TryInto};
use std::{mem::transmute, path::PathBuf};
use tendermint_proto::abci::{self, *};
type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

const STATE_SYNC_EPOCH: u64 = 1000;

struct MerkSnapshot {
    checkpoint: Merk,
    chunks: u32,
    hash: Vec<u8>,
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
    /// [`Merk`](https://docs.rs/merk/latest/merk/struct.Merk.html) instance.
    pub fn new(merk_home: PathBuf) -> Self {
        let merk = Merk::open(&merk_home.join("db")).unwrap();
        let restore_path = &merk_home.join("restore");
        if restore_path.exists() {
            std::fs::remove_dir_all(&restore_path).expect("Failed to remove state sync data");
        }
        MerkStore {
            map: Some(Default::default()),
            merk: Some(merk),
            home: merk_home,
            snapshots: BTreeMap::new(),
            target_snapshot: None,
            restorer: None,
        }
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

    pub(super) fn merk<'a>(&'a self) -> &'a Merk {
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

impl<'a> ABCIStore for MerkStore {
    fn height(&self) -> Result<u64> {
        let maybe_bytes = self.merk.as_ref().unwrap().get_aux(b"height")?;
        match maybe_bytes {
            None => Ok(0),
            Some(bytes) => Ok(read_u64(&bytes)),
        }
    }

    fn root_hash(&self) -> Result<Vec<u8>> {
        Ok(self.merk.as_ref().unwrap().root_hash().to_vec())
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        let height_bytes = height.to_be_bytes();

        let metadata = vec![(b"height".to_vec(), Some(height_bytes.to_vec()))];

        self.write(metadata)?;
        self.merk.as_mut().unwrap().flush()?;

        if self.height()? % STATE_SYNC_EPOCH == 0 {
            // Create new checkpoint
            let checkpoint = self
                .merk
                .as_ref()
                .unwrap()
                .checkpoint(self.home.join(self.height()?.to_string()))?;

            let snapshot = MerkSnapshot {
                hash: self.merk.as_ref().unwrap().root_hash().to_vec(),
                checkpoint,
                chunks: self.merk.as_ref().unwrap().chunks()?.len() as u32,
            };
            self.snapshots.insert(self.height()?, snapshot);

            if self.height()? > 2 * STATE_SYNC_EPOCH {
                let remove_height = self.height()? - 2 * STATE_SYNC_EPOCH;
                self.snapshots.remove(&remove_height);
                let path = self.home.join(remove_height.to_string());
                if path.exists() {
                    std::fs::remove_dir_all(path)?;
                }
            }
        }

        Ok(())
    }

    fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        let mut snapshots = vec![];
        // TODO: should we list all our snapshots,
        // or just the latest one?
        if let Some((height, snapshot)) = self.snapshots.last_key_value() {
            snapshots.push(Snapshot {
                chunks: snapshot.chunks,
                hash: snapshot.hash.clone(),
                height: *height,
                ..Default::default()
            });
        }

        Ok(snapshots)
    }

    fn load_snapshot_chunk(&self, req: RequestLoadSnapshotChunk) -> Result<Vec<u8>> {
        let mut chunk = vec![];
        if let Some(snapshot) = self.snapshots.get(&req.height) {
            chunk = snapshot.checkpoint.chunks()?.chunk(req.chunk as usize)?;
        }

        // TODO: what should we do if we haven't retained this snapshot?
        Ok(chunk)
    }

    fn apply_snapshot_chunk(
        &mut self,
        req: RequestApplySnapshotChunk,
    ) -> Result<ResponseApplySnapshotChunk> {
        let restore_path = self.home.join("restore");
        let target_snapshot = self
            .target_snapshot
            .as_mut()
            .expect("Tried to apply a snapshot chunk while no state sync is in progress");
        if let None = self.restorer {
            let expected_hash: [u8; 32] = target_snapshot
                .hash
                .clone()
                .try_into()
                .map_err(|_| format_err!("Failed to parse expected root hash"))?;
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
            let p = self.home.join("db");
            drop(restored);
            std::fs::rename(&restore_path, &p)?;
            self.merk = Some(Merk::open(&p)?);
        }

        Ok(Default::default())
    }

    fn offer_snapshot(&mut self, req: RequestOfferSnapshot) -> Result<ResponseOfferSnapshot> {
        let mut res = ResponseOfferSnapshot::default();
        res.set_result(abci::response_offer_snapshot::Result::Reject);
        if let Some(snapshot) = req.snapshot {
            if self.height()? + STATE_SYNC_EPOCH < snapshot.height
                && snapshot.height % STATE_SYNC_EPOCH == 0
                && snapshot.hash == req.app_hash
            {
                self.target_snapshot = Some(snapshot);
                res.set_result(abci::response_offer_snapshot::Result::Accept);
            }
        }

        Ok(res)
    }
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut array = [0; 8];
    array.copy_from_slice(&bytes);
    u64::from_be_bytes(array)
}
