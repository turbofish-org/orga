use super::*;
use std::collections::HashMap;

// TODO: should this be BTreeMap for efficient merging?
pub type Map = HashMap<Vec<u8>, Option<Vec<u8>>>;

pub type MapStore = WriteCache<NullStore>;

pub struct WriteCache<S: Store> {
    map: Map,
    store: S,
}

impl WriteCache<NullStore> {
    pub fn new() -> Self {
        WriteCache::wrap(NullStore)
    }
}

impl Default for WriteCache<NullStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Store> WriteCache<S> {
    pub fn wrap(store: S) -> Self {
        WriteCache {
            store,
            map: Default::default(),
        }
    }

    pub fn wrap_with_map(store: S, map: Map) -> Self {
        WriteCache { store, map }
    }

    pub fn into_map(self) -> Map {
        self.map
    }
}

impl<S: Store> Read for WriteCache<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key.as_ref()) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => self.store.get(key),
        }
    }
}

impl<S: Store> Write for WriteCache<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.map.insert(key.as_ref().to_vec(), None);
        Ok(())
    }
}

impl<S: Store> Flush for WriteCache<S> {
    fn flush(&mut self) -> Result<()> {
        for (key, value) in self.map.drain() {
            match value {
                Some(value) => self.store.put(key, value)?,
                None => self.store.delete(key.as_slice())?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satisfies_store_trait() {
        // (this is a compile-time assertion)
        fn assert_store<S: Store>(_: S) {}
        assert_store(WriteCache::new());
    }

    #[test]
    fn get_slice() {
        let mut store = WriteCache::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        let value = store.get(&[1, 2, 3]).unwrap();
        assert_eq!(value, Some(vec![4, 5, 6]));
    }

    #[test]
    fn delete() {
        let mut store = WriteCache::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        store.delete(&[1, 2, 3]).unwrap();
        assert_eq!(store.get(&[1, 2, 3]).unwrap(), None);
    }

    #[test]
    fn mapstore_new() {
        let _store: MapStore = MapStore::new();
    }
}
