use super::map::{Child, Map, MapIterator, ReadOnly};
use crate::collections;
use crate::encoding::{Decode, Encode};
use crate::state;
use crate::store::DefaultBackingStore;
use std::hash::Hash;
use std::ops::{Bound, Deref, DerefMut, RangeBounds};

use super::{Entry, Next};
use crate::state::*;
use crate::store::*;
use ed::*;

pub struct EntryMap<T: Entry, S = DefaultBackingStore> {
    map: Map<T::Key, T::Value, S>,
}

impl<T, S> EntryMap<T, S>
where
    T: Entry,
    T::Key: Encode + Terminated + Eq + Hash + Ord + Copy,
    T::Value: State<S>,
    S: Read,
{
    fn create(store: Store<S>) -> Result<Self> {
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

    fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        let val = self.map.entry(key)?.or_insert(value.into())?;

        Ok(())
    }

    fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _) = entry.into_entry();
        self.map.remove(key)?;

        Ok(())
    }

    fn contains(&self, entry: T) -> Result<bool> {
        let (key, _) = entry.into_entry();
        self.map.contains_key(key)
    }
}

impl<'a, T: Entry, S> EntryMap<T, S>
where
    T::Key: Next<T::Key> + Decode + Encode + Terminated + Hash + Eq + Ord + Copy,
    T::Value: State<S> + Copy,
    S: Read,
{
    fn iter(&'a mut self) -> EntryMapIterator<'a, T, S> {
        EntryMapIterator {
            map_iter: self.map.iter(),
        }
    }
}

pub struct EntryMapIterator<'a, T: Entry, S>
where
    T::Key: Next<T::Key> + Decode + Encode + Terminated + Hash + Eq,
    T::Value: State<S>,
    S: Read,
{
    map_iter: MapIterator<'a, T::Key, T::Value, S>,
}

impl<'a, T: Entry, S> Iterator for EntryMapIterator<'a, T, S>
where
    T::Key: Next<T::Key> + Decode + Encode + Terminated + Hash + Eq + Ord + Copy,
    T::Value: State<S> + Copy,
    S: Read,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let map_next: Option<(T::Key, Child<'a, T::Value>)> = self.map_iter.next();
        match map_next {
            Some((key, value)) => Some(T::from_entry((key, *value))),
            None => None,
        }
    }
}

mod test {
    use super::*;

    pub struct MapEntry {
        key: u32,
        value: u32,
    }

    impl Entry for MapEntry {
        type Key = u32;
        type Value = u32;

        fn into_entry(self) -> (Self::Key, Self::Value) {
            (self.key, self.value)
        }

        fn from_entry(entry: (Self::Key, Self::Value)) -> Self {
            MapEntry {
                key: entry.0,
                value: entry.1,
            }
        }
    }

    #[test]
    fn insert() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone()).unwrap();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();

        assert!(entry_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn insert_store() {
        let store = Store::new(MapStore::new());
        let mut edit_entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone()).unwrap();

        edit_entry_map
            .insert(MapEntry { key: 42, value: 84 })
            .unwrap();

        edit_entry_map.flush().unwrap();

        let read_entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone()).unwrap();
        assert!(read_entry_map
            .contains(MapEntry { key: 42, value: 84 })
            .unwrap());
    }

    #[test]
    fn delete_map() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone()).unwrap();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();
        entry_map.delete(MapEntry { key: 42, value: 84 }).unwrap();

        assert!(!entry_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn delete_store() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone()).unwrap();

        let entry = MapEntry { key: 42, value: 84 };
        entry_map.insert(entry).unwrap();
        entry_map.delete(MapEntry { key: 42, value: 84 }).unwrap();

        entry_map.flush().unwrap();

        let read_map: EntryMap<MapEntry> = EntryMap::create(store.clone()).unwrap();

        assert!(!read_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }
}
