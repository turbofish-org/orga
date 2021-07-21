use super::map::{Map, ReadOnly};
use crate::encoding::{Decode, Encode};
use crate::store::DefaultBackingStore;

use super::Entry;

pub struct EntryMap<T: Entry, S = DefaultBackingStore> {
    map: Map<T::Key, T::Value, S>,
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
