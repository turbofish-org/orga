use std::cell::Cell;
use std::collections::HashSet;
use super::*;

pub struct KeyLog<S: Store> {
    // TODO: since most keys are in both sets, we can dedupe and use a hash of
    // slices
    read_keys: Cell<HashSet<Vec<u8>>>,
    write_keys: HashSet<Vec<u8>>,
    store: S
}

impl<S: Store> KeyLog<S> {
    pub fn wrap(store: S) -> Self {
        KeyLog {
            read_keys: Cell::new(Default::default()),
            write_keys: Default::default(),
            store
        }
    }

    pub fn finish(self) -> (HashSet<Vec<u8>>, HashSet<Vec<u8>>) {
        (self.read_keys.into_inner(), self.write_keys)
    }
}

impl<S: Store> Read for KeyLog<S> {
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>> {
        let mut read_keys = self.read_keys.take();
        read_keys.insert(key.as_ref().to_vec());
        self.read_keys.set(read_keys);

        self.store.get(key)
    }
}

impl<S: Store> Write for KeyLog<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.write_keys.insert(key.clone());
        self.store.put(key, value)
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<()> {
        self.write_keys.insert(key.as_ref().to_vec());
        self.store.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satisfies_store_trait() {
        // (this is a compile-time assertion)
        fn assert_store<S: Store>(_: S) {}
        assert_store(KeyLog::wrap(NullStore));
    }

    #[test]
    fn get() {
        let mut store = WriteCache::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();

        let log = KeyLog::wrap(store);
        assert_eq!(log.get(vec![1, 2, 3]).unwrap(), Some(vec![4, 5, 6]));

        let (read_keys, write_keys) = log.finish();
        assert_eq!(read_keys.len(), 1);
        assert_eq!(write_keys.len(), 0);
        assert!(read_keys.contains(&vec![1, 2, 3]));
    }

    #[test]
    fn put() {
        let store = WriteCache::new();
        let mut log = KeyLog::wrap(store);
        log.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();

        let (read_keys, write_keys) = log.finish();
        assert_eq!(read_keys.len(), 0);
        assert_eq!(write_keys.len(), 1);
        assert!(write_keys.contains(&vec![1, 2, 3]));
    }

    #[test]
    fn delete() {
        let store = WriteCache::new();
        let mut log = KeyLog::wrap(store);
        log.delete(vec![1, 2, 3]).unwrap();

        let (read_keys, write_keys) = log.finish();
        assert_eq!(read_keys.len(), 0);
        assert_eq!(write_keys.len(), 1);
        assert!(write_keys.contains(&vec![1, 2, 3]));
    }
}