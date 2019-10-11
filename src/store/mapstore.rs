use std::collections::BTreeMap;
use super::*;

pub struct MapStore (BTreeMap<Vec<u8>, Vec<u8>>);

impl MapStore {
    pub fn new() -> MapStore {
        MapStore(BTreeMap::default())
    }
}

impl Read for MapStore {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Vec<u8>> {
        match self.0.get(key.as_ref()) {
            Some(value) => Ok(value.clone()),
            None => Err(Error::from(ErrorKind::NotFound).into())
        }
    }
}

impl Write for MapStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.0.insert(key, value);
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()> {
        self.0.remove(key.as_ref());
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
        assert_store(MapStore::new());
    }

    #[test]
    fn get_slice() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        let value = store.get(&[1, 2, 3]).unwrap();
        assert_eq!(value, vec![4, 5, 6]);
    }

    #[test]
    fn get_vec() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        let value = store.get(vec![1, 2, 3]).unwrap();
        assert_eq!(value, vec![4, 5, 6]);
    }

    #[test]
    fn delete() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        store.delete(vec![1, 2, 3]).unwrap();
        assert!(store.get(vec![1, 2, 3]).is_err());
    }
}