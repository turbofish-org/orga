use super::*;
use std::cell::Cell;
use std::collections::HashSet;

type Set = HashSet<Vec<u8>>;

// TODO: split into ReadLog and WriteLog
// TODO: use UnsafeCell instead of Cell since borrows can't last beyond the
// method calls so are guaranteed to be safe

/// A `Store` wrapper which records the keys of all reads and writes made
/// through it.
pub struct RWLog<S> {
    // TODO: since most keys are in both sets, we can dedupe and use a hash of
    // slices
    read_keys: Cell<Set>,
    write_keys: Set,
    store: S,
}

impl<S> RWLog<S> {
    pub fn wrap(store: S) -> Self {
        RWLog {
            read_keys: Cell::new(Default::default()),
            write_keys: Default::default(),
            store,
        }
    }

    pub fn finish(self) -> (Set, Set, S) {
        (self.read_keys.into_inner(), self.write_keys, self.store)
    }
}

impl<S: Read> Read for RWLog<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut read_keys = self.read_keys.take();
        read_keys.insert(key.as_ref().to_vec());
        self.read_keys.set(read_keys);

        self.store.get(key)
    }

    fn get_next(&self, _key: &[u8]) -> Result<Option<KV>> {
        // XXX: we will have to think about our concurrency rules wrt ranges of
        // keys, e.g. for a store with keys 1 and 3, get_next(1) should "lock"
        // for the range 1..3, and conflict with a write to 2
        todo!()
    }
}

impl<S: Write> Write for RWLog<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.write_keys.insert(key.clone());
        self.store.put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.write_keys.insert(key.as_ref().to_vec());
        self.store.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();

        let log = RWLog::wrap(store);
        assert_eq!(log.get(&[1, 2, 3]).unwrap(), Some(vec![4, 5, 6]));

        let (read_keys, write_keys, _store) = log.finish();
        assert_eq!(read_keys.len(), 1);
        assert_eq!(write_keys.len(), 0);
        assert!(read_keys.contains(&vec![1, 2, 3]));
    }

    #[test]
    fn get_missing() {
        let store = MapStore::new();

        let log = RWLog::wrap(store);
        assert_eq!(log.get(&[1, 2, 3]).unwrap(), None);

        let (read_keys, write_keys, _store) = log.finish();
        assert_eq!(read_keys.len(), 1);
        assert_eq!(write_keys.len(), 0);
        assert!(read_keys.contains(&vec![1, 2, 3]));
    }

    #[test]
    fn put() {
        let store = MapStore::new();
        let mut log = RWLog::wrap(store);
        log.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();

        let (read_keys, write_keys, _store) = log.finish();
        assert_eq!(read_keys.len(), 0);
        assert_eq!(write_keys.len(), 1);
        assert!(write_keys.contains(&vec![1, 2, 3]));
    }

    #[test]
    fn delete() {
        let store = MapStore::new();
        let mut log = RWLog::wrap(store);
        log.delete(&[1, 2, 3]).unwrap();

        let (read_keys, write_keys, _store) = log.finish();
        assert_eq!(read_keys.len(), 0);
        assert_eq!(write_keys.len(), 1);
        assert!(write_keys.contains(&vec![1, 2, 3]));
    }
}
