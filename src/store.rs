use crate::error::Result;

// TODO: Iterable trait so state machines can iterate through keys, or should this be required?
// TODO: Flush trait for stores that wrap a backing store

pub trait Read {
  fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Vec<u8>>;
}

pub trait Write {
  fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;
}

pub trait Store: Read + Write {}

impl<S: Read + Write> Store for S {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::*;

    struct MapStore (BTreeMap<Vec<u8>, Vec<u8>>);

    impl MapStore {
        fn new() -> MapStore {
            MapStore(BTreeMap::default())
        }
    }

    impl Read for MapStore {
        fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Vec<u8>> {
            match self.0.get(key.as_ref()) {
                Some(value) => Ok(value.clone()),
                None => Err(format_err!("not found"))
            }
        }
    }

    impl Write for MapStore {
        fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
            self.0.insert(key, value);
            Ok(())
        }
    }

    #[test]
    fn mapstore_satisfies_store_trait() {
        // (this is a compile-time assertion)
        fn assert_store<S: Store>(_: S) {}
        assert_store(MapStore::new());
    }

    #[test]
    fn mapstore_get_slice() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        let value = store.get(&[1, 2, 3]).unwrap();
        assert_eq!(value, vec![4, 5, 6]);
    }

    #[test]
    fn mapstore_get_vec() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        let value = store.get(vec![1, 2, 3]).unwrap();
        assert_eq!(value, vec![4, 5, 6]);
    }
}


