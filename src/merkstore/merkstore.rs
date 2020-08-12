use std::collections::BTreeMap;
use merk::{Merk, Op, BatchEntry};
use byteorder::{ByteOrder, BigEndian};
use crate::error::Result;
use crate::store::*;
use crate::abci::ABCIStore;

type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

/// A [`store::Store`] implementation backed by a [`merk`](https://docs.rs/merk)
/// Merkle key/value store.
pub struct MerkStore<'a> {
    merk: &'a mut Merk,
    map: Option<Map>
}

impl<'a> MerkStore<'a> {
    pub fn new(merk: &'a mut Merk) -> Self {
        MerkStore {
            map: Some(Default::default()),
            merk
        }
    }

    fn write(&mut self, aux: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Result<()> {
        let map = self.map.take().unwrap();
        self.map = Some(Map::new());

        let batch = to_batch(map);
        let aux_batch = to_batch(aux);

        Ok(self.merk.apply(batch.as_ref(), aux_batch.as_ref())?)
    }
}

fn to_batch<I: IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>)>>(i: I) -> Vec<BatchEntry> {
    let mut batch = Vec::new();
    for (key, val) in i {
        match val {
            Some(val) => batch.push((key, Op::Put(val))),
            None => batch.push((key, Op::Delete))
        }
    }
    batch
}

impl<'a> Read for MerkStore<'a> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.as_ref().unwrap().get(key) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => Ok(self.merk.get(key)?)
        }
    }
}

impl<'a> Write for MerkStore<'a> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.as_mut().unwrap().insert(key, Some(value));
        Ok(())
    }

    fn delete(&mut self, key:  &[u8]) -> Result<()> {
        self.map.as_mut().unwrap().insert(key.to_vec(), None);
        Ok(())
    }
}

impl<'a> Flush for MerkStore<'a> {
    fn flush(&mut self) -> Result<()> {
        self.write(vec![])
    }
}

impl<'a> ABCIStore for MerkStore<'a> {
    fn height(&self) -> Result<u64> {
        let maybe_bytes = self.merk.get_aux(b"height")?;
        match maybe_bytes {
            None => Ok(0),
            Some(bytes) => Ok(BigEndian::read_u64(&bytes))
        }
    }

    fn root_hash(&self) -> Result<Vec<u8>> {
        Ok(self.merk.root_hash().to_vec())
    }

    // TODO: we don't need the hash 
    fn query(&self, key: &[u8]) -> Result<Vec<u8>> {
        let val = &[key.to_vec()];
        let mut hash = self.root_hash()?;
        let data = self.merk.prove(val)?;
        hash.extend(data);
        Ok(hash)
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        let mut height_bytes = [0; 8];
        BigEndian::write_u64(&mut height_bytes, height);

        let metadata = vec![
            (b"height".to_vec(), Some(height_bytes.to_vec()))
        ];

        self.write(metadata)?;
        self.merk.flush()?;

        Ok(())
    }
}