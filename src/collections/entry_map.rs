use super::map::{Map, ReadOnly};
use crate::encoding::{Decode, Encode};
use crate::store::DefaultBackingStore;
use std::hash::Hash;
use std::ops::{Bound, Deref, DerefMut, RangeBounds};
use crate::collections;
use crate::state;

use super::Entry;
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
}
