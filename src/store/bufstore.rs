use std::cmp::Ordering;
use std::collections::{btree_map, BTreeMap};
use std::iter::Peekable;

use super::*;

/// An in-memory map containing values modified by writes to a `BufStore`.
pub type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

/// A simple `Store` implementation which persists data in an in-memory map.
pub type MapStore = BufStore<NullStore>;

/// Wraps a `Store` and records mutations in an in-memory map, so that
/// modifications do not affect the underlying `Store` until `flush` is called.
pub struct BufStore<S: Read> {
    map: Map,
    store: S,
}

impl<S: Read + Default> BufStore<S> {
    /// Constructs a `BufStore` which wraps the default value of the inner
    /// store.
    pub fn new() -> Self {
        Default::default()
    }
}

impl<S: Read + Default> Default for BufStore<S> {
    fn default() -> Self {
        Self {
            map: Default::default(),
            store: Default::default()
        }
    }
}

impl<S: Read> BufStore<S> {
    /// Constructs a `BufStore` by wrapping the given store.
    ///
    /// Calls to get will first check the `BufStore` map, and if no entry is
    /// found will be passed to the underlying store.
    pub fn wrap(store: S) -> Self {
        BufStore {
            store,
            map: Default::default(),
        }
    }

    /// Creates a `BufStore` by wrapping the given store, using a pre-populated
    /// in-memory buffer of key/value entries.
    pub fn wrap_with_map(store: S, map: Map) -> Self {
        BufStore { store, map }
    }

    /// Consumes the `BufStore` and returns its in-memory buffer of key/value
    /// entries.
    pub fn into_map(self) -> Map {
        self.map
    }
}

impl<S: Read> Read for BufStore<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key.as_ref()) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => self.store.get(key),
        }
    }
}

impl<S: Read> Write for BufStore<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.map.insert(key.as_ref().to_vec(), None);
        Ok(())
    }
}

impl<S: Store> Flush for BufStore<S> {
    /// Consumes the `BufStore`'s in-memory buffer and writes all of its values
    /// to the underlying store.
    ///
    /// After calling `flush`, the `BufStore` will still be valid and wrap the
    /// underlying store, but its in-memory buffer will be empty.
    fn flush(&mut self) -> Result<()> {
        while let Some((key, value)) = self.map.pop_first() {
            match value {
                Some(value) => self.store.put(key, value)?,
                None => self.store.delete(key.as_slice())?,
            }
        }
        Ok(())
    }
}

type MapIter<'a> = btree_map::Range<'a, Vec<u8>, Option<Vec<u8>>>;

impl<S> super::Iter for BufStore<S>
where
    S: Read + super::Iter
{
    type Iter<'a> = Iter<'a, S::Iter<'a>>;

    fn iter_from(&self, start: &[u8]) -> Self::Iter<'_> {
        let map_iter = self.map.range(start.to_vec()..);
        let backing_iter = self.store.iter_from(start);
        Iter {
            map_iter: map_iter.peekable(),
            backing_iter: backing_iter.peekable()
        }
    }
}

/// An iterator implementation over entries in a `BufStore`.
///
/// Entries will be emitted for values in the underlying store, reflecting the
/// modifications stored in the in-memory map.
pub struct Iter<'a, B>
where
    B: Iterator<Item = Entry<'a>>,
{
    map_iter: Peekable<MapIter<'a>>,
    backing_iter: Peekable<B>
}

impl<'a, B> Iterator for Iter<'a, B>
where
    B: Iterator<Item = Entry<'a>>,
{
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let has_map_entry = self.map_iter.peek().is_some();
            let has_backing_entry = self.backing_iter.peek().is_some();

            return match (has_map_entry, has_backing_entry) {
                // consumed both iterators, end here
                (false, false) => None,

                // consumed backing iterator, still have map values
                (true, false) => {
                    match self.map_iter.next().unwrap() {
                        // map value is not a delete, emit value
                        (key, Some(value)) => Some((key.as_slice(), value.as_slice())),
                        // map value is a delete, go to next entry
                        (_, None) => continue,
                    }
                }

                // consumed map iterator, still have backing values
                (false, true) => self.backing_iter.next(),

                // merge values from both iterators
                (true, true) => {
                    let map_key = self.map_iter.peek().unwrap().0;
                    let backing_key = self.backing_iter.peek().unwrap().0;
                    let key_cmp = map_key.as_slice().cmp(backing_key);

                    // map key > backing key, emit backing entry
                    if key_cmp == Ordering::Greater {
                        let entry = self.backing_iter.next().unwrap();
                        return Some(entry);
                    }

                    // map key == backing key, map entry shadows backing entry
                    if key_cmp == Ordering::Equal {
                        self.backing_iter.next();
                    }

                    // map key <= backing key, emit map entry (or skip if delete)
                    match self.map_iter.next().unwrap() {
                        (key, Some(value)) => Some((key.as_slice(), value.as_slice())),
                        (_, None) => continue,
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Iter;

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
        assert_eq!(value, Some(vec![4, 5, 6]));
    }

    #[test]
    fn delete() {
        let mut store = MapStore::new();
        store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
        store.delete(&[1, 2, 3]).unwrap();
        assert_eq!(store.get(&[1, 2, 3]).unwrap(), None);
    }

    #[test]
    fn mapstore_new() {
        let _store: MapStore = MapStore::new();
    }

    #[test]
    fn iter() {
        let mut store = MapStore::new();
        store.put(vec![0], vec![0]).unwrap();
        store.put(vec![1], vec![0]).unwrap();
        store.put(vec![2], vec![0]).unwrap();
        store.put(vec![4], vec![0]).unwrap();

        let mut buf = BufStore::wrap(store);
        buf.put(vec![1], vec![1]).unwrap();
        buf.delete(&[2]).unwrap();
        buf.put(vec![3], vec![1]).unwrap();

        let mut iter = buf.iter();
        assert_eq!(iter.next(), Some((&[0][..], &[0][..])));
        assert_eq!(iter.next(), Some((&[1][..], &[1][..])));
        assert_eq!(iter.next(), Some((&[3][..], &[1][..])));
        assert_eq!(iter.next(), Some((&[4][..], &[0][..])));
        assert_eq!(iter.next(), None);
    }
}
