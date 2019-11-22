use merk::{Merk, Op};
use std::collections::BTreeMap;
use crate::error::Result;
use crate::store::*;

// use BTreeMap???
type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

pub struct MerkStore<'a> {
    merk: &'a mut Merk,
    map: Map
}

impl<'a> MerkStore<'a> {
    pub fn new(merk: &'a mut Merk) -> Self {
        MerkStore {
            map: Default::default(),
            merk
        }
    }
}

// get from the map, if it doesn't exist, get from merk
impl<'a> Read for MerkStore<'a> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => Ok(self.merk.get(key)?)
        }
    }
}

// write and delete from map
impl<'a> Write for MerkStore<'a> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }
    fn delete(&mut self, key:  &[u8]) -> Result<()> {
        self.map.insert(key.to_vec(), None);
        Ok(())
    }
}

impl<'a> Flush for MerkStore<'a> {
    fn flush(self) -> Result<()> {
        let mut batch = Vec::new();
        for (key, val) in self.map {
            match val {
                Some(val) => batch.push((key, Op::Put(val))),
                None => batch.push((key, Op::Delete))
            }
        }
        self.merk.apply(batch.as_ref())?;
        Ok(())
    }
}