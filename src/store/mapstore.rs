use std::collections::BTreeMap;
use super::*;

type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

pub struct MapStore<'a, R: Read> {
    map: Map,
    store: &'a R
}

pub struct MapFlusher (Map);

impl MapStore<'_, NullStore> {
    pub fn new() -> Self {
        MapStore::wrap(&NullStore)
    }
}

impl<'a, R: Read> MapStore<'a, R> {
    pub fn wrap(store: &'a R) -> Self {
        MapStore {
            map: Default::default(),
            store
        }
    }

    pub fn finish(self) -> MapFlusher {
        MapFlusher(self.map)
    }
}

impl<'a, S: Read> Read for MapStore<'a, S> {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Vec<u8>> {
        match self.map.get(key.as_ref()) {
            Some(Some(value)) => Ok(value.clone()),
            Some(None) => Err(Error::from(ErrorKind::NotFound).into()),
            None => self.store.get(key)
        }
    }
}

impl<'a, S: Read> Write for MapStore<'a, S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()> {
        // TODO: remove if key only exists in map
        self.map.insert(key.as_ref().to_vec(), None);
        Ok(())
    }
}

impl Flush for MapFlusher {
    fn flush<W: Write>(self, dest: &mut W) -> Result<()> {
        for (key, value) in self.0 {
            match value {
                Some(value) => dest.put(key, value)?,
                None => dest.delete(key)?
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