use super::map::Iter as MapIter;
use super::map::Map;
use super::map::ReadOnly;

use crate::encoding::{Decode, Encode};
use crate::store::DefaultBackingStore;
use std::ops::RangeBounds;

use super::{Entry, Next};
use crate::call::Call;
use crate::query::Query;
use crate::state::*;
use crate::store::*;
use crate::Result;
use ed::*;

#[derive(Query, Call)]
pub struct EntryMap<T: Entry, S = DefaultBackingStore> {
    map: Map<T::Key, T::Value, S>,
}

impl<T: Entry, S> From<EntryMap<T, S>> for () {
    fn from(_map: EntryMap<T, S>) {}
}

impl<T: Entry, S> State<S> for EntryMap<T, S>
where
    T::Key: Encode + Terminated,
    T::Value: State<S>,
{
    type Encoding = ();

    fn create(store: Store<S>, _: ()) -> Result<Self>
    where
        S: Read,
    {
        Ok(EntryMap {
            map: Map::create(store, ())?,
        })
    }

    fn flush(self) -> Result<()>
    where
        S: Write,
    {
        self.map.flush()
    }
}

// TODO: add a get_mut method (maybe just takes in T::Key?) so we can add
// #[call] to it to route calls to children

impl<T, S> EntryMap<T, S>
where
    T: Entry,
    T::Key: Encode + Terminated,
    T::Value: State<S>,
    S: Read,
{
    pub fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        self.map.insert(key, value.into())
    }

    #[query]
    pub fn contains_entry_key(&self, entry: T) -> Result<bool> {
        let (key, _) = entry.into_entry();
        self.map.contains_key(key)
    }
}

impl<T, S> EntryMap<T, S>
where
    T: Entry,
    T::Key: Encode + Terminated + Clone,
    T::Value: State<S>,
    S: Read,
{
    pub fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _) = entry.into_entry();
        self.map.remove(key)?;

        Ok(())
    }
}

impl<T, S> EntryMap<T, S>
where
    T: Entry,
    T::Key: Encode + Terminated + Clone,
    T::Value: State<S> + Eq,
    S: Read,
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

impl<'a, T: Entry, S> EntryMap<T, S>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
    T::Value: State<S> + Clone,
    S: Read,
{
    pub fn iter(&'a mut self) -> Result<Iter<'a, T, S>> {
        Ok(Iter {
            map_iter: self.map.iter()?,
        })
    }

    pub fn range<B: RangeBounds<T::Key>>(&'a mut self, range: B) -> Result<Iter<'a, T, S>> {
        Ok(Iter {
            map_iter: self.map.range(range)?,
        })
    }
}

pub struct Iter<'a, T: Entry, S>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
    T::Value: State<S> + Clone,
    S: Read,
{
    map_iter: MapIter<'a, T::Key, T::Value, S>,
}

impl<'a, T: Entry, S> Iterator for Iter<'a, T, S>
where
    T::Key: Next + Decode + Encode + Terminated + Clone,
    T::Value: State<S> + Clone,
    S: Read,
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

mod test {
    use super::{EntryMap as OrgaEntryMap, *};
    #[allow(dead_code)]
    type EntryMap<T> = OrgaEntryMap<T, MapStore>;

    #[derive(Entry, Debug, Eq, PartialEq)]
    pub struct MapEntry {
        #[key]
        key: u32,
        value: u32,
    }

    #[derive(Entry, Debug, Eq, PartialEq)]
    pub struct TupleMapEntry(#[key] u32, u32);

    #[test]
    fn insert() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();

        assert!(entry_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn insert_store() {
        let store = Store::new(MapStore::new());
        let mut edit_entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        edit_entry_map
            .insert(MapEntry { key: 42, value: 84 })
            .unwrap();

        edit_entry_map.flush().unwrap();

        let read_entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();
        assert!(read_entry_map
            .contains(MapEntry { key: 42, value: 84 })
            .unwrap());
    }

    #[test]
    fn delete_map() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();
        entry_map.delete(MapEntry { key: 42, value: 84 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn delete_store() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();
        entry_map.delete(MapEntry { key: 42, value: 84 }).unwrap();

        entry_map.flush().unwrap();

        let read_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        assert!(!read_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn iter() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

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
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

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
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

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
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

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
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 12, value: 13 }).unwrap());
    }

    #[test]
    fn contains_removed_entry() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.delete(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 12, value: 24 }).unwrap());
    }

    #[test]
    fn contains_entry_key() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(entry_map
            .contains_entry_key(MapEntry { key: 12, value: 24 })
            .unwrap());
    }

    #[test]
    fn contains_entry_key_value_non_match() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store, ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();

        assert!(entry_map
            .contains_entry_key(MapEntry { key: 12, value: 13 })
            .unwrap());
    }

    #[test]
    fn iter_tuple_struct() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<TupleMapEntry> = EntryMap::create(store.clone(), ()).unwrap();

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
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<TupleMapEntry> = EntryMap::create(store.clone(), ()).unwrap();

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
}
