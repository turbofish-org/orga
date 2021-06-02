use std::collections::{BTreeMap, btree_map};
use std::cmp::Ordering;

use super::*;

/// An in-memory map containing values modified by writes to a `BufStore`.
pub type Map = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

/// A simple `Store` implementation which persists data in an in-memory map.
pub type MapStore = BufStore<NullStore>;

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
    
    /// Consumes the `BufStore`'s in-memory buffer and writes all of its values
    /// to the underlying store.
    ///
    /// After calling `flush`, the `BufStore` will still be valid and wrap the
    /// underlying store, but its in-memory buffer will be empty.
    #[inline]
    pub fn flush(&mut self) -> Result<()>
        where S: Write
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
        match self.map.get(key.as_ref()) {
            Some(Some(value)) => Ok(Some(value.clone())),
            Some(None) => Ok(None),
            None => self.store.get(key),
        }
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        // TODO: optimize by retaining previously used iterator(s) so we don't
        // have to recreate them each iteration (if it makes a difference)
        let mut map_iter = self.map.range(exclusive_range_from(key));
        let mut store_iter = self.store.range(exclusive_range_from(key));
        iter_merge_next(&mut map_iter, &mut store_iter)
    }

    #[inline]
    fn get_prev(&self, key: &[u8]) -> Result<Option<KV>> {
        todo!()
        // let mut map_iter = self.map.range(key.to_vec()..).rev();
        // let mut store_iter = self.store.range(key.to_vec()..).rev();
        // iter_merge_next(&mut map_iter, &mut store_iter)
    }
}

fn exclusive_range_from(start: &[u8]) -> (Bound<Vec<u8>>, Bound<Vec<u8>>) {
    (
        Bound::Excluded(start.to_vec()),
        Bound::Unbounded,
    )
}

fn iter_merge_next<S: Read>(
    map_iter: &mut btree_map::Range<Vec<u8>, Option<Vec<u8>>>,
    store_iter: &mut Iter<S>,
) -> Result<Option<KV>> {
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
                    (key, Some(value)) => Some((key.clone(), value.clone())),
                    // map value is a delete, go to next entry
                    (_, None) => continue,
                }
            }

            // consumed map iterator, still have backing values
            (false, true) => store_iter.next().transpose()?,

            // merge values from both iterators
            (true, true) => {
                let map_key = map_iter.peek().unwrap().0;
                let backing_key = match store_iter.peek().unwrap() {
                    Err(err) => failure::bail!("{}", err),
                    Ok((ref key, _)) => key,
                };
                let key_cmp = map_key.cmp(backing_key);

                // map key > backing key, emit backing entry
                if key_cmp == Ordering::Greater {
                    let entry = store_iter.next().unwrap()?;
                    return Ok(Some(entry));
                }

                // map key == backing key, map entry shadows backing entry
                if key_cmp == Ordering::Equal {
                    store_iter.next();
                }

                // map key <= backing key, emit map entry (or skip if delete)
                match map_iter.next().unwrap() {
                    (key, Some(value)) => Some((key.clone(), value.clone())),
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
        self.map.insert(key.as_ref().to_vec(), None);
        Ok(())
    }
}

// impl<'a, B> Iterator for Iter<'a, B>
// where
//     B: Iterator<Item = Entry>,
// {
//     type Item = Entry;

//     fn next(&mut self) -> Option<Self::Item> {
//         
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use super::super::Iter;

//     #[test]
//     fn satisfies_store_trait() {
//         // (this is a compile-time assertion)
//         fn assert_store<S: Store>(_: S) {}
//         assert_store(MapStore::new());
//     }

//     #[test]
//     fn get_slice() {
//         let mut store = MapStore::new();
//         store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
//         let value = store.get(&[1, 2, 3]).unwrap();
//         assert_eq!(value, Some(vec![4, 5, 6]));
//     }

//     #[test]
//     fn delete() {
//         let mut store = MapStore::new();
//         store.put(vec![1, 2, 3], vec![4, 5, 6]).unwrap();
//         store.delete(&[1, 2, 3]).unwrap();
//         assert_eq!(store.get(&[1, 2, 3]).unwrap(), None);
//     }

//     #[test]
//     fn mapstore_new() {
//         let _store: MapStore = MapStore::new();
//     }

//     #[test]
//     fn iter() {
//         let mut store = MapStore::new();
//         store.put(vec![0], vec![0]).unwrap();
//         store.put(vec![1], vec![0]).unwrap();
//         store.put(vec![2], vec![0]).unwrap();
//         store.put(vec![4], vec![0]).unwrap();

//         let mut buf = BufStore::wrap(store);
//         buf.put(vec![1], vec![1]).unwrap();
//         buf.delete(&[2]).unwrap();
//         buf.put(vec![3], vec![1]).unwrap();

//         let mut iter = buf.iter();
//         assert_eq!(iter.next(), Some((&[0][..], &[0][..])));
//         assert_eq!(iter.next(), Some((&[1][..], &[1][..])));
//         assert_eq!(iter.next(), Some((&[3][..], &[1][..])));
//         assert_eq!(iter.next(), Some((&[4][..], &[0][..])));
//         assert_eq!(iter.next(), None);
//     }
// }
