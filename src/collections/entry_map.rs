//! A collection for types which can convert to/from a key/value pair.
use serde::Serialize;

use super::map::Iter as MapIter;
use super::map::Map;
use super::map::ReadOnly;

use crate::describe::Describe;
use crate::encoding::{Decode, Encode, Terminated};
use crate::migrate::Migrate;
use std::ops::RangeBounds;

use super::{Entry, Next};
use crate::call::FieldCall;
use crate::query::FieldQuery;
use crate::state::*;
use crate::store::*;
use crate::Result;

/// A collection for types which can be converted to/from a key/value pair.
///
/// EntryMap is backed by a [Map], where both the key and value are derived from
/// the inserted value itself.
///
/// This collection is often useful when the data can be naturally thought of as
/// a set or priority queue.
#[derive(FieldQuery, FieldCall, Encode, Decode, Describe, Serialize)]
#[serde(bound = "T::Key: Serialize + Terminated + Clone, T::Value: Serialize")]
pub struct EntryMap<T: Entry> {
    map: Map<T::Key, T::Value>,
}

impl<T: Entry> State for EntryMap<T>
where
    T::Key: Encode + Terminated + 'static,
    T::Value: State,
    Self: 'static,
{
    fn attach(&mut self, store: Store) -> Result<()> {
        self.map.attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        self.map.flush(out)
    }

    fn load(store: Store, _bytes: &mut &[u8]) -> Result<Self> {
        let mut entry_map = EntryMap::default();
        entry_map.map.attach(store)?;

        Ok(entry_map)
    }
}

// impl<T: Entry + 'static> Describe for EntryMap<T>
// where
//     T::Key: Encode + Terminated + Describe + 'static,
//     T::Value: State + Describe + 'static,
// {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>()
//             .named_child::<Map<T::Key, T::Value>>("map", &[], |v| {
//                 crate::describe::Builder::access(v, |v: Self| v.map)
//             })
//             .build()
//     }
// }

impl<T: Entry> Default for EntryMap<T> {
    fn default() -> Self {
        Self {
            map: Map::default(),
        }
    }
}

impl<T: Entry> EntryMap<T> {
    /// Create a new, empty [EntryMap].
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Entry> EntryMap<T>
where
    T::Key: Encode + Terminated + 'static,
    T::Value: State,
{
    /// Create a new [EntryMap] with the given backing [Store].
    pub fn with_store(store: Store) -> Result<Self> {
        Ok(Self {
            map: Map::with_store(store)?,
        })
    }
}

// TODO: add a get_mut method (maybe just takes in T::Key?) so we can add
// #[call] to it to route calls to children

impl<T> EntryMap<T>
where
    T: Entry,
    T::Key: Encode + Terminated + 'static,
    T::Value: State,
{
    /// Insert an entry.
    pub fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        self.map.insert(key, value)
    }
}

impl<T> EntryMap<T>
where
    T: Entry,
    T::Key: Encode + Terminated + Clone + 'static,
    T::Value: State,
{
    /// Remove an entry.
    pub fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _) = entry.into_entry();
        self.map.remove(key)?;

        Ok(())
    }
}

// #[orga]
impl<T> EntryMap<T>
where
    T: Entry,
    T::Key: Encode + Terminated + Clone + 'static,
    T::Value: State + Eq,
{
    // #[query]
    /// Check if the map contains an entry.
    pub fn contains(&self, entry: T) -> Result<bool> {
        let (key, value) = entry.into_entry();

        match self.map.contains_key(key.clone())? {
            true => {
                let map_value = match self.map.get(key)? {
                    Some(val) => val,
                    None => {
                        return Ok(false);
                    }
                };

                Ok(*map_value == value)
            }
            false => Ok(false),
        }
    }

    // TODO: this query can be moved to an impl with more permissive bounds
    // after some query macro changes
    // #[query]
    /// Check if the map contains an entry with a key matching the one computed
    /// by the provided entry.
    pub fn contains_entry_key(&self, entry: T) -> Result<bool> {
        let (key, _) = entry.into_entry();
        self.map.contains_key(key)
    }
}

impl<'a, T: Entry> EntryMap<T>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
    T::Value: State + Clone,
{
    /// Create an iterator over the entries.
    pub fn iter(&'a self) -> Result<Iter<'a, T>> {
        Ok(Iter {
            map_iter: self.map.iter()?,
        })
    }

    /// Create an iterator over the entries within a given range.
    pub fn range<B: RangeBounds<T::Key>>(&'a self, range: B) -> Result<Iter<'a, T>> {
        Ok(Iter {
            map_iter: self.map.range(range)?,
        })
    }
}

/// An iterator over the entries of an [EntryMap].
pub struct Iter<'a, T: Entry>
where
    T::Key: Next + Decode + Encode + Terminated + Clone + 'static,
    T::Value: State + Clone,
{
    map_iter: MapIter<'a, T::Key, T::Value>,
}

impl<'a, T: Entry> Iterator for Iter<'a, T>
where
    T::Key: Next + Decode + Encode + Terminated + Clone + 'static,
    T::Value: State + Clone,
{
    type Item = Result<ReadOnly<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        let map_next = self.map_iter.next();
        map_next.map(|entry| match entry {
            Ok((key, value)) => Ok(ReadOnly::new(T::from_entry((
                (*key).clone(),
                (*value).clone(),
            )))),
            Err(err) => Err(err),
        })
    }
}

impl<T> Migrate for EntryMap<T>
where
    T: Entry,
    T::Key: Encode + Terminated + Migrate + Clone,
    T::Value: State + Migrate,
{
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            map: Map::migrate(src, dest, bytes)?,
        })
    }
}

#[cfg(all(test, feature = "merk"))]
mod tests {
    use crate::store::BackingStore;

    use super::*;

    #[derive(Entry, Debug, Eq, PartialEq)]
    pub struct MapEntry {
        #[key]
        key: u32,
        value: u32,
    }

    #[derive(Entry, Debug, Eq, PartialEq)]
    pub struct TupleMapEntry(#[key] u32, u32);

    fn setup<T: Entry>() -> (Store, EntryMap<T>)
    where
        T::Key: Terminated + 'static,
        T::Value: State,
    {
        let backing_store = BackingStore::MapStore(Shared::new(MapStore::new()));
        let store = Store::new(backing_store);
        let em = EntryMap::with_store(store.clone()).unwrap();
        (store, em)
    }

    #[test]
    fn insert() {
        let (_store, mut entry_map) = setup();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();

        assert!(entry_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn insert_store() {
        let (store, mut edit_entry_map) = setup();

        edit_entry_map
            .insert(MapEntry { key: 42, value: 84 })
            .unwrap();

        let mut buf = vec![];
        edit_entry_map.flush(&mut buf).unwrap();

        let mut read_entry_map: EntryMap<MapEntry> = Default::default();
        read_entry_map.attach(store).unwrap();
        assert!(read_entry_map
            .contains(MapEntry { key: 42, value: 84 })
            .unwrap());
    }

    #[test]
    fn delete_map() {
        let (_store, mut entry_map) = setup();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();
        entry_map.delete(MapEntry { key: 42, value: 84 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn delete_store() {
        let (store, mut entry_map) = setup();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();
        entry_map.delete(MapEntry { key: 42, value: 84 }).unwrap();

        let mut buf = vec![];
        entry_map.flush(&mut buf).unwrap();

        let read_map: EntryMap<MapEntry> = EntryMap::with_store(store).unwrap();

        assert!(!read_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn iter() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let actual: Vec<MapEntry> = vec![
            MapEntry { key: 12, value: 24 },
            MapEntry { key: 13, value: 26 },
            MapEntry { key: 14, value: 28 },
        ];

        let result: bool = entry_map
            .iter()
            .unwrap()
            .zip(actual.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }

    #[test]
    fn range_full() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let expected_entries: Vec<MapEntry> = vec![
            MapEntry { key: 12, value: 24 },
            MapEntry { key: 13, value: 26 },
            MapEntry { key: 14, value: 28 },
        ];

        let result: bool = entry_map
            .range(..)
            .unwrap()
            .zip(expected_entries.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }

    #[test]
    fn range_exclusive_upper() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let expected_entries: Vec<MapEntry> = vec![
            MapEntry { key: 12, value: 24 },
            MapEntry { key: 13, value: 26 },
        ];

        let result: bool = entry_map
            .range(..14)
            .unwrap()
            .zip(expected_entries.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }

    #[test]
    fn range_bounded_exclusive() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let expected_entries: Vec<MapEntry> = vec![MapEntry { key: 13, value: 26 }];

        let result: bool = entry_map
            .range(13..14)
            .unwrap()
            .zip(expected_entries.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }

    #[test]
    fn contains_wrong_entry() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 12, value: 13 }).unwrap());
    }

    #[test]
    fn contains_removed_entry() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.delete(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 12, value: 24 }).unwrap());
    }

    #[test]
    fn contains_entry_key() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(entry_map
            .contains_entry_key(MapEntry { key: 12, value: 24 })
            .unwrap());
    }

    #[test]
    fn contains_entry_key_value_non_match() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(entry_map
            .contains_entry_key(MapEntry { key: 12, value: 13 })
            .unwrap());
    }

    #[test]
    fn iter_tuple_struct() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(TupleMapEntry(12, 24)).unwrap();
        entry_map.insert(TupleMapEntry(13, 26)).unwrap();
        entry_map.insert(TupleMapEntry(14, 28)).unwrap();

        let actual: Vec<TupleMapEntry> = vec![
            TupleMapEntry(12, 24),
            TupleMapEntry(13, 26),
            TupleMapEntry(14, 28),
        ];

        let result: bool = entry_map
            .iter()
            .unwrap()
            .zip(actual.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }

    #[test]
    fn range_full_tuple_struct() {
        let (_store, mut entry_map) = setup();

        entry_map.insert(TupleMapEntry(12, 24)).unwrap();
        entry_map.insert(TupleMapEntry(13, 26)).unwrap();
        entry_map.insert(TupleMapEntry(14, 28)).unwrap();

        let expected_entries: Vec<TupleMapEntry> = vec![
            TupleMapEntry(12, 24),
            TupleMapEntry(13, 26),
            TupleMapEntry(14, 28),
        ];

        let result: bool = entry_map
            .range(..)
            .unwrap()
            .zip(expected_entries.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }

    #[derive(Entry, Debug, Eq, PartialEq)]
    pub struct MultiKeyMapEntry {
        #[key]
        key_1: u32,
        #[key]
        key_2: u8,
        #[key]
        key_3: u16,
        value: u32,
    }

    #[test]
    fn insert_multi_key() {
        let (_store, mut entry_map) = setup();

        let entry = MultiKeyMapEntry {
            key_1: 42,
            key_2: 12,
            key_3: 9,
            value: 84,
        };

        entry_map.insert(entry).unwrap();

        assert!(entry_map
            .contains(MultiKeyMapEntry {
                key_1: 42,
                key_2: 12,
                key_3: 9,
                value: 84
            })
            .unwrap());
    }

    #[test]
    fn delete_multi_key() {
        let (_store, mut entry_map) = setup();

        let entry = MultiKeyMapEntry {
            key_1: 42,
            key_2: 12,
            key_3: 9,
            value: 84,
        };

        entry_map.insert(entry).unwrap();
        entry_map
            .delete(MultiKeyMapEntry {
                key_1: 42,
                key_2: 12,
                key_3: 9,
                value: 84,
            })
            .unwrap();

        assert!(!entry_map
            .contains(MultiKeyMapEntry {
                key_1: 42,
                key_2: 12,
                key_3: 9,
                value: 84
            })
            .unwrap());
    }

    #[test]
    fn iter_multi_key() {
        let (store, mut entry_map) = setup();

        entry_map
            .insert(MultiKeyMapEntry {
                key_1: 0,
                key_2: 0,
                key_3: 1,
                value: 1,
            })
            .unwrap();
        entry_map
            .insert(MultiKeyMapEntry {
                key_1: 1,
                key_2: 0,
                key_3: 1,
                value: 9,
            })
            .unwrap();
        entry_map
            .insert(MultiKeyMapEntry {
                key_1: 0,
                key_2: 1,
                key_3: 0,
                value: 4,
            })
            .unwrap();

        let mut buf = vec![];
        entry_map.flush(&mut buf).unwrap();

        let expected: Vec<MultiKeyMapEntry> = vec![
            MultiKeyMapEntry {
                key_1: 0,
                key_2: 0,
                key_3: 1,
                value: 1,
            },
            MultiKeyMapEntry {
                key_1: 0,
                key_2: 1,
                key_3: 0,
                value: 4,
            },
            MultiKeyMapEntry {
                key_1: 1,
                key_2: 0,
                key_3: 1,
                value: 9,
            },
        ];

        let entry_map: EntryMap<MultiKeyMapEntry> = EntryMap::with_store(store).unwrap();
        let result: bool = entry_map
            .iter()
            .unwrap()
            .zip(expected.iter())
            .map(|(actual, expected)| *actual.unwrap() == *expected)
            .fold(true, |accumulator, item| item & accumulator);

        assert!(result);
    }
}
