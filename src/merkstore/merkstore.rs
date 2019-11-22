extern crate merk;
use merk::*;
use std::collections::HashMap;
use error_chain::bail;

// use BTreeMap???
type Map = HashMap<Vec<u8>, Option<Vec<u8>>>;

pub struct MerkStore<'a> {
    merk: &'a mut M,
    map: Map
}

impl MerkStore<'a> {
    pub fn new() -> Self {
        let mut M = Merk::open("./merk.db").unwrap();
        MerkStore::wrap(&'a mut M)
    }
}

impl<'a> MerkStore<'a> {
    pub fn wrap(merk: &'a mut M) -> Self {
        MerkStore {
            map: Default::default(); //idk why this is here?
            merk
        }
    }
}
// maybe i don't need this???? 
impl Default for MerkStore {
    fn default () -> Self {
        Self::new()
    }
}

// get from the map, if it doesn't exist, get from merk
impl<'a, M: Store> Read for MerkStore<'a, M> {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => {
                match self.merk.get(key)? {
                    Some(val) => Ok(val), // self.merk.get(key) returns Result<Vec<u8>>
                    None => bail!("No value found for key")
                }
            }
        }
    }
}

// write and delete from map
impl<'a, M: Store> Write for MerkStore<'a> {
    pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }
    pub fn delete(&mut self, key:  &[u8]) -> Result<()> {
        self.map.insert(key.to_vec(), None);
        Ok(())
    }
}

impl<'a> Flush for MerkStore<'a> {
    fn flush(&mut self) -> Result<()> {
        let mut batch = Vec::new();
        for (key, val) in self.map.drain() {
            match val {
                Some(val) => batch.push(key, merk::Op::Put(val)),
                None => batch.push(key, merk::Op::Delete)
            }
        }
        self.merk.apply(batch).unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
}
