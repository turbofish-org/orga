use super::map::{Child, Map, MapIterator};
use crate::encoding::{Decode, Encode};
use crate::store::DefaultBackingStore;
use std::hash::Hash;
use std::ops::RangeBounds;

use super::{Entry, Next};
use crate::state::*;
use crate::store::*;
use ed::*;

pub struct EntryMap<T: Entry, S = DefaultBackingStore> {
    map: Map<T::Key, T::Value, S>,
}

impl<T: Entry, S> From<EntryMap<T, S>> for () {
    fn from(_map: EntryMap<T, S>) {}
}

impl<T: Entry, S> State<S> for EntryMap<T, S>
where
    T::Key: Encode + Terminated + Eq + Hash + Ord,
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

impl<T, S> EntryMap<T, S>
where
    T: Entry,
    T::Key: Encode + Terminated + Eq + Hash + Ord + Copy,
    T::Value: State<S>,
    S: Read,
{
    pub fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        self.map.entry(key)?.or_insert(value.into())?;

        Ok(())
    }

    pub fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _) = entry.into_entry();
        self.map.remove(key)?;

        Ok(())
    }

    pub fn contains(&self, entry: T) -> Result<bool> {
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
    pub fn iter(&'a mut self) -> EntryMapIterator<'a, T, S> {
        EntryMapIterator {
            map_iter: self.map.iter(),
        }
    }

    pub fn range<B: RangeBounds<T::Key> + Clone>(
        &'a mut self,
        range: B,
    ) -> EntryMapIterator<'a, T, S> {
        EntryMapIterator {
            map_iter: self.map.range(range),
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
        map_next.map(|(key, value)| T::from_entry((key, *value)))
    }
}

mod test {
    use super::*;

    #[derive(Debug, Eq, PartialEq)]
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
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

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

        let read_entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();
        assert!(read_entry_map
            .contains(MapEntry { key: 42, value: 84 })
            .unwrap());
    }

    #[test]
    fn delete_map() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

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

        let read_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        assert!(!read_map.contains(MapEntry { key: 42, value: 84 }).unwrap());
    }

    #[test]
    fn iter() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let mut expected: Vec<MapEntry> = Vec::with_capacity(3);
        entry_map.iter().for_each(|entry| expected.push(entry));

        let actual: Vec<MapEntry> = vec![
            MapEntry { key: 12, value: 24 },
            MapEntry { key: 13, value: 26 },
            MapEntry { key: 14, value: 28 },
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn range_full() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let mut expected: Vec<MapEntry> = Vec::with_capacity(3);
        entry_map.range(..).for_each(|entry| expected.push(entry));

        let actual: Vec<MapEntry> = vec![
            MapEntry { key: 12, value: 24 },
            MapEntry { key: 13, value: 26 },
            MapEntry { key: 14, value: 28 },
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn range_exclusive_upper() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let mut expected: Vec<MapEntry> = Vec::with_capacity(3);
        entry_map.range(..14).for_each(|entry| expected.push(entry));

        let actual: Vec<MapEntry> = vec![
            MapEntry { key: 12, value: 24 },
            MapEntry { key: 13, value: 26 },
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn range_bounded_exclusive() {
        let store = Store::new(MapStore::new());
        let mut entry_map: EntryMap<MapEntry> = EntryMap::create(store.clone(), ()).unwrap();

        entry_map.insert(MapEntry { key: 12, value: 24 }).unwrap();
        entry_map.insert(MapEntry { key: 13, value: 26 }).unwrap();
        entry_map.insert(MapEntry { key: 14, value: 28 }).unwrap();

        let mut expected: Vec<MapEntry> = Vec::with_capacity(3);
        entry_map
            .range(13..14)
            .for_each(|entry| expected.push(entry));

        let actual: Vec<MapEntry> = vec![MapEntry { key: 13, value: 26 }];

        assert_eq!(actual, expected);
    }
}
