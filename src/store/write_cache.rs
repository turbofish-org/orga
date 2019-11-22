use std::collections::HashMap;
use super::*;

// TODO: should this be BTreeMap for efficient merging?
type Map = HashMap<Vec<u8>, Option<Vec<u8>>>;

pub type MapStore = WriteCache<'static, NullStore>;

pub struct WriteCache<'a, S: Store> {
    map: Map,
    store: &'a mut S
}

impl WriteCache<'_, NullStore> {
    pub fn new() -> Self {
        WriteCache::wrap(unsafe { &mut NULL_STORE })
    }
}

impl Default for WriteCache<'_, NullStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, S: Store> WriteCache<'a, S> {
    pub fn wrap(store: &'a mut S) -> Self {
        WriteCache {
            map: Default::default(),
            store
        }
    }
}

impl<'a, S: Store> Read for WriteCache<'a, S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key.as_ref()) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => self.store.get(key)
        }
    }
}

impl<'a, S: Store> Write for WriteCache<'a, S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.map.insert(key.as_ref().to_vec(), None);
        Ok(())
    }
}

impl<'a, S: Store> Flush for WriteCache<'a, S> {
    fn flush(&mut self) -> Result<()> {
        for (key, value) in self.map.drain() {
            match value {
                Some(value) => self.store.put(key, value)?,
                None => self.store.delete(key.as_slice())?
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