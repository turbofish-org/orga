//! A store which buffers writes to another store.
use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::*;
use crate::Error;

/// An in-memory map containing values modified by writes to a `BufStore`.
pub type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

/// A simple `Store` implementation which persists data in an in-memory map.
pub type MapStore = BufStore<Empty>;

/// Wraps a `Store` and records mutations in an in-memory map, so that
/// modifications do not affect the underlying `Store` until `flush` is called.
pub struct BufStore<S> {
    map: Map,
    store: S,
}

impl<S: Read + Default> BufStore<S> {
    /// Constructs a `BufStore` which wraps the default value of the inner
    /// store.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }
}

impl<S: Read + Default> Default for BufStore<S> {
    #[inline]
    fn default() -> Self {
        Self {
            map: Default::default(),
            store: Default::default(),
        }
    }
}

impl<S> BufStore<S> {
    /// Constructs a `BufStore` by wrapping the given store.
    ///
    /// Calls to get will first check the `BufStore` map, and if no entry is
    /// found will be passed to the underlying store.
    #[inline]
    pub fn wrap(store: S) -> Self {
        BufStore {
            store,
            map: Default::default(),
        }
    }

    /// Creates a `BufStore` by wrapping the given store, using a pre-populated
    /// in-memory buffer of key/value entries.
    #[inline]
    pub fn wrap_with_map(store: S, map: Map) -> Self {
        BufStore { store, map }
    }

    /// Consumes the `BufStore` and returns its in-memory buffer of key/value
    /// entries.
    #[inline]
    pub fn into_map(self) -> Map {
        self.map
    }

    /// Returns the a reference to the underlying store.
    #[inline]
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Consumes the `BufStore`'s in-memory buffer and writes all of its values
    /// to the underlying store.
    ///
    /// After calling `flush`, the `BufStore` will still be valid and wrap the
    /// underlying store, but its in-memory buffer will be empty.
    #[inline]
    pub fn flush(&mut self) -> Result<()>
    where
        S: Write,
    {
        // TODO: use drain instead of pop?
        while let Some((key, value)) = self.map.pop_first() {
            match value {
                Some(value) => self.store.put(key, value)?,
                None => self.store.delete(key.as_slice())?,
            }
        }
        Ok(())
    }
}

impl<S: Read> Read for BufStore<S> {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => self.store.get(key),
        }
    }

    // TODO: optimize by retaining previously used iterator(s) so we don't
    // have to recreate them each iteration (if it makes a difference)

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let mut map_iter = self
            .map
            .range(exclusive_range_starting_from(key))
            .map(|(k, v)| (k.clone(), v.clone()));
        let mut store_iter = (&self.store).into_iter(exclusive_range_starting_from(key));
        iter_merge_next(&mut map_iter, &mut store_iter, true)
    }

    #[inline]
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        let range = || {
            key.map_or((Bound::Unbounded, Bound::Unbounded), |key| {
                exclusive_range_ending_at(key)
            })
        };
        let mut map_iter = self
            .map
            .range(range())
            .rev()
            .map(|(k, v)| (k.clone(), v.clone()));
        let mut store_iter = (&self.store).into_iter(range()).rev();
        iter_merge_next(&mut map_iter, &mut store_iter, false)
    }
}

/// Return range bounds which start from the given key (exclusive), with an
/// unbounded end.
fn exclusive_range_starting_from(start: &[u8]) -> (Bound<Vec<u8>>, Bound<Vec<u8>>) {
    (Bound::Excluded(start.to_vec()), Bound::Unbounded)
}

fn exclusive_range_ending_at(start: &[u8]) -> (Bound<Vec<u8>>, Bound<Vec<u8>>) {
    (Bound::Unbounded, Bound::Excluded(start.to_vec()))
}

/// Takes an iterator over entries in the in-memory map and an iterator over
/// entries in the backing store, and yields the next entry. Entries in the map
/// shadow entries in the backing store with the same key, including skipping
/// entries marked as deleted (a `None` value in the map).
fn iter_merge_next<M, S>(
    map_iter: &mut M,
    store_iter: &mut S,
    increasing: bool,
) -> Result<Option<KV>>
where
    M: Iterator<Item = (Vec<u8>, Option<Vec<u8>>)>,
    S: Iterator<Item = Result<KV>>,
{
    let mut map_iter = map_iter.peekable();
    let mut store_iter = store_iter.peekable();

    loop {
        let has_map_entry = map_iter.peek().is_some();
        let has_backing_entry = store_iter.peek().is_some();

        return Ok(match (has_map_entry, has_backing_entry) {
            // consumed both iterators, end here
            (false, false) => None,

            // consumed backing iterator, still have map values
            (true, false) => {
                match map_iter.next().unwrap() {
                    // map value is not a delete, emit value
                    (key, Some(value)) => Some((key, value)),
                    // map value is a delete, go to next entry
                    (_, None) => continue,
                }
            }

            // consumed map iterator, still have backing values
            (false, true) => store_iter.next().transpose()?,

            // merge values from both iterators
            (true, true) => {
                let map_key = &map_iter.peek().unwrap().0;
                let backing_key = match store_iter.peek().unwrap() {
                    Err(_) => return Err(Error::Store("Backing key does not exist".into())),
                    Ok((ref key, _)) => key,
                };
                let key_cmp = map_key.cmp(backing_key);

                // map key is past backing key, emit backing entry
                if (increasing && key_cmp == Ordering::Greater)
                    || (!increasing && key_cmp == Ordering::Less)
                {
                    let entry = store_iter.next().unwrap()?;
                    return Ok(Some(entry));
                }

                // map key == backing key, map entry shadows backing entry
                if key_cmp == Ordering::Equal {
                    store_iter.next();
                }

                // map key is before or at backing key, emit map entry (or skip if delete)
                match map_iter.next().unwrap() {
                    (key, Some(value)) => Some((key, value)),
                    (_, None) => continue,
                }
            }
        });
    }
}

impl<S: Read> Write for BufStore<S> {
    #[inline]
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.map.insert(key, Some(value));
        Ok(())
    }

    #[inline]
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.map.insert(key.to_vec(), None);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        buf.put(vec![5], vec![1]).unwrap();

        let mut iter = buf.into_iter(..);
        assert_eq!(iter.next().unwrap().unwrap(), (vec![0], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![3], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![4], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![5], vec![1]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn rev_iter() {
        let mut store = MapStore::new();
        store.put(vec![1], vec![0]).unwrap();
        store.put(vec![2], vec![0]).unwrap();
        store.put(vec![3], vec![0]).unwrap();
        store.put(vec![4], vec![0]).unwrap();

        let mut buf = BufStore::wrap(store);
        buf.put(vec![0], vec![1]).unwrap();
        buf.put(vec![2], vec![1]).unwrap();
        buf.delete(&[3]).unwrap();
        buf.put(vec![5], vec![1]).unwrap();

        let mut iter = buf.into_iter(..).rev();
        assert_eq!(iter.next().unwrap().unwrap(), (vec![5], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![4], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![0], vec![1]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn wrap_with_map_and_flush() {
        let mut store = Shared::new(MapStore::new());
        store.put(vec![0], vec![100]).unwrap();
        store.put(vec![1], vec![101]).unwrap();

        let mut wrapped = BufStore::wrap_with_map(store.clone(), Default::default());
        assert_eq!(wrapped.get(&[0]).unwrap().unwrap(), vec![100]);

        wrapped.put(vec![0], vec![102]).unwrap();
        wrapped.delete(&[1]).unwrap();
        wrapped.put(vec![2], vec![103]).unwrap();

        assert_eq!(wrapped.get(&[0]).unwrap().unwrap(), vec![102]);
        assert!(wrapped.get(&[1]).unwrap().is_none());
        assert_eq!(wrapped.get(&[2]).unwrap().unwrap(), vec![103]);
        assert_eq!(store.get(&[0]).unwrap().unwrap(), vec![100]);
        assert_eq!(store.get(&[1]).unwrap().unwrap(), vec![101]);
        assert!(store.get(&[2]).unwrap().is_none());

        wrapped.flush().unwrap();

        assert_eq!(store.get(&[0]).unwrap().unwrap(), vec![102]);
        assert!(store.get(&[1]).unwrap().is_none());
        assert_eq!(store.get(&[2]).unwrap().unwrap(), vec![103]);
    }

    #[test]
    fn into_map() {
        let mut buf = BufStore::wrap(MapStore::new());
        buf.put(vec![0], vec![100]).unwrap();
        let mut map = buf.into_map();

        assert_eq!(map.remove(&vec![0]), Some(Some(vec![100])));
    }
}
