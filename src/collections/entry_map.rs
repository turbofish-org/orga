use super::map::Iter as MapIter;
use super::map::Map;
use super::map::ReadOnly;
use serde::Deserialize;
use serde::Serialize;

use crate::encoding::{Decode, Encode, Terminated};
use crate::migrate::{MigrateFrom, MigrateInto};
use std::ops::RangeBounds;

use super::{Entry, Next};
use crate::call::Call;
use crate::describe::Describe;
use crate::query::Query;
use crate::state::*;
use crate::store::*;
use crate::Result;

#[derive(Query, Call, Encode, Decode, Serialize, Deserialize)]
#[serde(bound(
    serialize = "T::Key: Serialize + Terminated + Clone, T::Value: Serialize + State",
    deserialize = "T::Key: Deserialize<'de> + Terminated + Clone, T::Value: Deserialize<'de> + State",
))]
pub struct EntryMap<T: Entry> {
    map: Map<T::Key, T::Value>,
}

impl<T: Entry> State for EntryMap<T>
where
    T::Key: Encode + Terminated,
    T::Value: State,
{
    fn attach(&mut self, store: Store) -> Result<()> {
        self.map.attach(store)
    }

    fn flush(&mut self) -> Result<()> {
        self.map.flush()
    }
}

impl<T: Entry + 'static> Describe for EntryMap<T>
where
    T::Key: Encode + Terminated + Describe + 'static,
    T::Value: State + Describe + 'static,
{
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>()
            .named_child::<Map<T::Key, T::Value>>("map", &[], |v| {
                crate::describe::Builder::access(v, |v: Self| v.map)
            })
            .build()
    }
}

impl<T: Entry> Default for EntryMap<T> {
    fn default() -> Self {
        Self {
            map: Map::default(),
        }
    }
}

impl<T: Entry> EntryMap<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Entry> EntryMap<T>
where
    T::Key: Encode + Terminated,
    T::Value: State,
{
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
    T::Key: Encode + Terminated,
    T::Value: State,
{
    pub fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        self.map.insert(key, value)
    }

    #[query]
    pub fn contains_entry_key(&self, entry: T) -> Result<bool> {
        let (key, _) = entry.into_entry();
        self.map.contains_key(key)
    }
}

impl<T> EntryMap<T>
where
    T: Entry,
    T::Key: Encode + Terminated + Clone,
    T::Value: State,
{
    pub fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _) = entry.into_entry();
        self.map.remove(key)?;

        Ok(())
    }
}

impl<T> EntryMap<T>
where
    T: Entry,
    T::Key: Encode + Terminated + Clone,
    T::Value: State + Eq,
{
    #[query]
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
}

impl<'a, T: Entry> EntryMap<T>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
    T::Value: State + Clone,
{
    pub fn iter(&'a self) -> Result<Iter<'a, T>> {
        Ok(Iter {
            map_iter: self.map.iter()?,
        })
    }

    pub fn range<B: RangeBounds<T::Key>>(&'a self, range: B) -> Result<Iter<'a, T>> {
        Ok(Iter {
            map_iter: self.map.range(range)?,
        })
    }
}

pub struct Iter<'a, T: Entry>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
    T::Value: State + Clone,
{
    map_iter: MapIter<'a, T::Key, T::Value>,
}

impl<'a, T: Entry> Iterator for Iter<'a, T>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
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

impl<T1, T2> MigrateFrom<EntryMap<T1>> for EntryMap<T2>
where
    T1: Entry,
    T1::Key: Next + Decode + Encode + Terminated + Clone,
    T1::Value: State + Clone,
    T2: Entry + MigrateFrom<T1>,
    T2::Key: Encode + Terminated,
    T2::Value: State,
{
    fn migrate_from(other: EntryMap<T1>) -> Result<Self> {
        // TODO: clone bound shouldn't be required on T1::Value, but there's currently no
        // way to access the inner map's store, so we need the
        // clone bound to create the entry map's iterator
        let mut entry_map = Self::default();
        for entry in other.map.iter()? {
            let (k, v) = entry?;
            let entry = T1::from_entry((k.clone(), v.clone()));
            entry_map.insert(entry.migrate_into()?)?;
        }

        Ok(entry_map)
    }
}

#[cfg(all(test, feature = "merk"))]
mod tests {
    use crate::merk::BackingStore;

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
        T::Key: Terminated,
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

        edit_entry_map.flush().unwrap();

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

        entry_map.flush().unwrap();

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
        entry_map.flush().unwrap();

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
