use merk::{Merk, Op, Hash};
use std::collections::BTreeMap;
use crate::error::Result;
use crate::store::*;

type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

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
        let map = self.map.take().unwrap();
        self.map = Some(Map::new());
        let mut batch = Vec::new();
        for (key, val) in map {
            match val {
                Some(val) => batch.push((key, Op::Put(val))),
                None => batch.push((key, Op::Delete))
            }
        }
        self.merk.apply(batch.as_ref())?;
        Ok(())
    }
}

impl<'a> RootHash for MerkStore<'a> {
    fn root_hash(&self) -> Vec<u8> {
        self.merk.root_hash().to_vec()
    }
}

impl<'a> Query for MerkStore<'a> {
    fn query(&mut self, key: &[u8]) -> Result<Vec<u8>> {
        let val = &[key.to_vec()];
        Ok(self.merk.prove(val)?)
    }
}