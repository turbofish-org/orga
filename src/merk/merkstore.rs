use crate::abci::ABCIStore;
use crate::error::Result;
use crate::store::*;
use merk::tree::Tree;
use merk::{rocksdb, BatchEntry, Merk, Op};
use std::collections::BTreeMap;
use std::mem::transmute;

type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

/// A [`store::Store`] implementation backed by a [`merk`](https://docs.rs/merk)
/// Merkle key/value store.
pub struct MerkStore<'a> {
    merk: &'a mut Merk,
    map: Option<Map>,
}

impl<'a> MerkStore<'a> {
    /// Constructs a `MerkStore` which references the given
    /// [`Merk`](https://docs.rs/merk/latest/merk/struct.Merk.html) instance.
    pub fn new(merk: &'a mut Merk) -> Self {
        MerkStore {
            map: Some(Default::default()),
            merk,
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

        Ok(self.merk.apply(batch.as_ref(), aux_batch.as_ref())?)
    }

    pub(super) fn merk(&self) -> &Merk {
        self.merk
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

impl<'a> Read for MerkStore<'a> {
    /// Gets a value from the underlying `Merk` store.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.as_ref().unwrap().get(key) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => Ok(self.merk.get(key)?),
        }
    }

    fn get_next(&self, start: &[u8]) -> Result<Option<KV>> {
        // TODO: use an iterator in merk which steps through in-memory nodes
        // (loading if necessary)
        let mut iter = self.merk.raw_iter();
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

impl<'a> Write for MerkStore<'a> {
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

impl<'a> ABCIStore for MerkStore<'a> {
    fn height(&self) -> Result<u64> {
        let maybe_bytes = self.merk.get_aux(b"height")?;
        match maybe_bytes {
            None => Ok(0),
            Some(bytes) => Ok(read_u64(&bytes)),
        }
    }

    fn root_hash(&self) -> Result<Vec<u8>> {
        Ok(self.merk.root_hash().to_vec())
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        let height_bytes = height.to_be_bytes();

        let metadata = vec![(b"height".to_vec(), Some(height_bytes.to_vec()))];

        self.write(metadata)?;
        self.merk.flush()?;

        Ok(())
    }
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut array = [0; 8];
    array.copy_from_slice(&bytes);
    u64::from_be_bytes(array)
}
