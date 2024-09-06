//! Read-only snapshots of a Merk instance.
use crate::store::Read;
use crate::Result;
use merk::{Hash, Merk};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tendermint_proto::v0_34::abci::{RequestLoadSnapshotChunk, Snapshot as AbciSnapshot};

use super::store::{FIRST_SNAPSHOT_HEIGHT, SNAPSHOT_INTERVAL};

/// A snapshot of a [Merk].
///
/// These snapshots are offered to peers via Tendermint to support state
/// sync.
#[derive(Clone)]
pub struct Snapshot {
    pub(crate) checkpoint: Arc<RwLock<Merk>>,
    length: u32,
    hash: Hash,
}

impl Snapshot {
    fn new(checkpoint: Merk) -> Result<Self> {
        let length = {
            let chunks = checkpoint.chunks()?;
            chunks.len() as u32
        };

        let hash = checkpoint.root_hash();

        Ok(Self {
            checkpoint: Arc::new(RwLock::new(checkpoint)),
            length,
            hash,
        })
    }

    fn chunk(&self, index: usize) -> Result<Vec<u8>> {
        let checkpoint = self.checkpoint.read().unwrap();
        // TODO: refactor ChunkProducer in Merk to not retain reference to db,
        // so we can reuse it across chunks rather than creating a new
        // ChunkProducer each time
        let chunk = checkpoint.chunks()?.chunk(index)?;
        Ok(chunk)
    }
}

impl Read for Snapshot {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.checkpoint.read().unwrap().get(key)?)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<crate::store::KV>> {
        let cp = self.checkpoint.read().unwrap();
        let iter = cp.raw_iter();
        super::store::get_next(iter, key)
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<crate::store::KV>> {
        let cp = self.checkpoint.read().unwrap();
        let iter = cp.raw_iter();
        super::store::get_prev(iter, key)
    }
}

/// A filter applied to snapshots to determine whether they should be created or
/// retained.
pub enum SnapshotFilter {
    /// Retain a snapshot at most once every `interval` blocks, up to `limit`
    /// (dropping older snapshots).
    Interval {
        /// The interval between snapshots, in number of blocks.
        interval: u64,
        /// The maximum number of snapshots this filter may retain.
        limit: u64,
    },
    /// Retain a snapshot of a specific height.
    SpecificHeight {
        /// A snapshot of this height will be retained.
        height: u64,
        /// The height at which this filter will no longer be applied, if
        /// provided.
        keep_until: Option<u64>,
    },
}

impl SnapshotFilter {
    /// Create an interval filter.
    pub fn interval(interval: u64, limit: u64) -> Self {
        SnapshotFilter::Interval { interval, limit }
    }

    /// Create a specific height filter.
    pub fn specific_height(height: u64, keep_until: Option<u64>) -> Self {
        SnapshotFilter::SpecificHeight { height, keep_until }
    }

    /// Determine whether a snapshot should be created at the given height.
    pub fn should_create(&self, height: u64) -> bool {
        match self {
            SnapshotFilter::Interval { interval, .. } => height % interval == 0,
            SnapshotFilter::SpecificHeight { height: h, .. } => height == *h,
        }
    }

    /// Determine whether a snapshot should be retained at the given height.
    pub fn should_keep(&self, ss_height: u64, cur_height: u64) -> bool {
        match self {
            SnapshotFilter::Interval { interval, limit } => {
                ss_height % interval == 0 && cur_height - ss_height < interval * limit
            }
            SnapshotFilter::SpecificHeight { height, keep_until } => {
                ss_height == *height && keep_until.map_or(true, |n| cur_height < n)
            }
        }
    }
}

/// A collection of snapshots.
#[derive(Default)]
pub struct Snapshots {
    snapshots: BTreeMap<u64, Snapshot>,
    filters: Vec<SnapshotFilter>,
    path: PathBuf,
}

impl Snapshots {
    /// Create a new snapshot collection, and ensure the storage directory
    /// exists.
    pub fn new(path: &Path) -> Result<Self> {
        if !path.exists() {
            std::fs::create_dir(path).expect("Failed to create snapshot directory");
        }

        Ok(Self {
            snapshots: BTreeMap::new(),
            filters: vec![],
            path: path.to_path_buf(),
        })
    }

    /// Load a snapshot collection from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let mut snapshots = Self::new(path)?;

        let snapshot_dir = snapshots.path.read_dir()?;
        for entry in snapshot_dir {
            let entry = entry?;
            let path = entry.path();

            // TODO: open read-only
            let checkpoint = Merk::open(&path)?;
            let snapshot = Snapshot::new(checkpoint)?;

            let height_str = path.file_name().unwrap().to_str().unwrap();
            let height: u64 = height_str.parse()?;
            snapshots.snapshots.insert(height, snapshot);
        }

        Ok(snapshots)
    }

    /// Add filters to this snapshot collection.
    pub fn with_filters(mut self, filters: Vec<SnapshotFilter>) -> Self {
        self.filters = filters;
        self
    }

    /// Get a snapshot at the given height, if it exists.
    pub fn get(&self, height: u64) -> Option<&Snapshot> {
        self.snapshots.get(&height)
    }

    /// Get the latest snapshot and its height, if it exists.
    pub fn get_latest(&self) -> Option<(u64, &Snapshot)> {
        self.snapshots.iter().next_back().map(|(h, s)| (*h, s))
    }

    /// Determine whether a snapshot should be created at the given height.
    pub fn should_create(&self, height: u64) -> bool {
        height > 0 && self.filters.iter().any(|f| f.should_create(height))
    }

    /// Determine whether a snapshot should be retained at the given height.
    pub fn should_keep(&self, ss_height: u64, cur_height: u64) -> bool {
        self.filters
            .iter()
            .any(|f| f.should_keep(ss_height, cur_height))
    }

    /// Create a snapshot at the given height.
    pub fn create(&mut self, height: u64, checkpoint: Merk) -> Result<()> {
        if self.snapshots.contains_key(&height) {
            return Ok(());
        }

        let snapshot = Snapshot::new(checkpoint)?;
        self.snapshots.insert(height, snapshot);

        self.maybe_prune(height)
    }

    /// Prune snapshots that are no longer needed.
    pub fn maybe_prune(&mut self, cur_height: u64) -> Result<()> {
        let remove_heights = self
            .snapshots
            .iter()
            .filter_map(|(ss_height, _)| {
                if self.should_keep(*ss_height, cur_height) {
                    None
                } else {
                    Some(*ss_height)
                }
            })
            .collect::<Vec<_>>();

        for ss_height in remove_heights {
            self.snapshots.remove(&ss_height);

            let path = self.path(ss_height);
            if path.exists() {
                std::fs::remove_dir_all(path)?;
            }
        }

        Ok(())
    }

    /// Returns the path to a snapshot at the given height.
    pub fn path(&self, height: u64) -> PathBuf {
        self.path.join(height.to_string())
    }

    /// Returns the ABCI snapshots to offer to a peer.
    pub fn abci_snapshots(&self) -> Result<Vec<AbciSnapshot>> {
        self.snapshots
            .iter()
            .filter(|(height, _)| {
                *height % SNAPSHOT_INTERVAL == 0 || **height == FIRST_SNAPSHOT_HEIGHT
            })
            .map(|(height, snapshot)| {
                Ok(AbciSnapshot {
                    chunks: snapshot.length,
                    hash: snapshot.hash.to_vec().into(),
                    height: *height,
                    ..Default::default()
                })
            })
            .collect()
    }

    /// Load a chunk from a snapshot.
    pub fn abci_load_chunk(&self, req: RequestLoadSnapshotChunk) -> Result<Vec<u8>> {
        match self.snapshots.get(&req.height) {
            Some(snapshot) => snapshot.chunk(req.chunk as usize),
            // ABCI has no way to specify that we don't have the requested
            // chunk, so we just return an empty one (and probably get banned by
            // the client when they try to verify)
            None => Ok(vec![]),
        }
    }
}
