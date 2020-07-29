use std::cmp::Ordering;
use std::collections::{btree_map, BTreeMap};
use std::iter::Peekable;

use super::*;

pub type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

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

// TODO: implement generically for all backing stores
impl<'a> super::Iter<'a, 'a> for WriteCache<NullStore> {
    type Iter = Iter<'a, 'static, super::nullstore::NullIter>;

    fn iter(&'a self, start: &[u8]) -> Self::Iter {
        let map_iter = self.map.range(start.to_vec()..);
        let backing_iter = self.store.iter(start);
        Iter {
            map_iter: map_iter.peekable(),
            backing_iter: backing_iter.peekable(),
        }
    }
}

pub struct Iter<'a, 'b: 'a, B>
where
    B: Iterator<Item = (&'b [u8], &'b [u8])>,
{
    map_iter: Peekable<MapIter<'a>>,
    backing_iter: Peekable<B>,
}

impl<'a, 'b: 'a, B> Iterator for Iter<'a, 'b, B>
where
    B: Iterator<Item = (&'b [u8], &'b [u8])>,
{
    type Item = (&'a [u8], &'a [u8]);

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
                        (key, Some(value)) => Some((key, value.as_slice())),
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
