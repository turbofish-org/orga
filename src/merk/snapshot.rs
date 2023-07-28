use crate::store::Read;
use crate::Result;
use merk::{chunks::ChunkProducer, Hash, Merk};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::mem::transmute;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tendermint_proto::v0_34::abci::{RequestLoadSnapshotChunk, Snapshot as AbciSnapshot};

use super::store::{FIRST_SNAPSHOT_HEIGHT, SNAPSHOT_INTERVAL};

#[derive(Clone)]
pub struct Snapshot {
    pub(crate) checkpoint: Rc<RefCell<Merk>>,
    chunks: Rc<RefCell<Option<ChunkProducer<'static>>>>,
    length: u32,
    hash: Hash,
}

impl Snapshot {
    fn new(checkpoint: Merk) -> Result<Self> {
        let chunks = checkpoint.chunks()?;
        let chunks: ChunkProducer<'static> = unsafe { transmute(chunks) };

        let length = chunks.len() as u32;
        let hash = checkpoint.root_hash();

        Ok(Self {
            checkpoint: Rc::new(RefCell::new(checkpoint)),
            chunks: Rc::new(RefCell::new(Some(chunks))),
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

impl Read for Snapshot {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.checkpoint.borrow().get(key)?)
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<crate::store::KV>> {
        super::store::get_next(&self.checkpoint.borrow(), key)
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<crate::store::KV>> {
        super::store::get_prev(&self.checkpoint.borrow(), key)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        // drop the self-referential ChunkProducer before the Merk instance
        self.chunks.borrow_mut().take();
    }
}

pub enum SnapshotFilter {
    Interval {
        interval: u64,
        limit: u64,
    },
    SpecificHeight {
        height: u64,
        keep_until: Option<u64>,
    },
}

impl SnapshotFilter {
    pub fn interval(interval: u64, limit: u64) -> Self {
        SnapshotFilter::Interval { interval, limit }
    }

    pub fn specific_height(height: u64, keep_until: Option<u64>) -> Self {
        SnapshotFilter::SpecificHeight { height, keep_until }
    }

    pub fn should_create(&self, height: u64) -> bool {
        match self {
            SnapshotFilter::Interval { interval, .. } => height % interval == 0,
            SnapshotFilter::SpecificHeight { height: h, .. } => height == *h,
        }
    }

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

#[derive(Default)]
pub struct Snapshots {
    snapshots: BTreeMap<u64, Snapshot>,
    filters: Vec<SnapshotFilter>,
    path: PathBuf,
}

impl Snapshots {
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

    pub fn with_filters(mut self, filters: Vec<SnapshotFilter>) -> Self {
        self.filters = filters;
        self
    }

    pub fn get(&self, height: u64) -> Option<&Snapshot> {
        self.snapshots.get(&height)
    }

    pub fn get_latest(&self) -> Option<(u64, &Snapshot)> {
        self.snapshots.iter().next_back().map(|(h, s)| (*h, s))
    }

    pub fn should_create(&self, height: u64) -> bool {
        height > 0 && self.filters.iter().any(|f| f.should_create(height))
    }

    pub fn should_keep(&self, ss_height: u64, cur_height: u64) -> bool {
        self.filters
            .iter()
            .any(|f| f.should_keep(ss_height, cur_height))
    }

    pub fn create(&mut self, height: u64, checkpoint: Merk) -> Result<()> {
        if self.snapshots.contains_key(&height) {
            return Ok(());
        }

        let snapshot = Snapshot::new(checkpoint)?;
        self.snapshots.insert(height, snapshot);

        self.maybe_prune(height)
    }

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

    pub fn path(&self, height: u64) -> PathBuf {
        self.path.join(height.to_string())
    }

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
