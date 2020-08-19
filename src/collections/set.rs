use std::borrow::Borrow;

use super::Map;
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::{Read, Store};
use crate::Result;

/// A set data structure.
pub struct Set<S: Read, T: Encode + Decode> {
    map: Map<S, T, ()>,
}

impl<S: Read, T: Encode + Decode> State<S> for Set<S, T> {
    /// Constructs a `Set` which is backed by the given store.
    fn wrap_store(store: S) -> Result<Self> {
        Ok(Self {
            map: Map::wrap_store(store)?,
        })
    }
}

impl<S: Read, T: Encode + Decode> Set<S, T> {
    /// Return true if the set contains the given value, or false otherwise.
    ///
    /// If an error is encountered while accessing the store, it will be
    /// returned.
    pub fn contains<U: Borrow<T>>(&self, value: U) -> Result<bool> {
        Ok(self.map.get(value)?.is_some())
    }
}

impl<S: Store, T: Encode + Decode> Set<S, T> {
    /// Inserts the given value into the set. If the value is already in the
    /// set, this is a no-op.
    ///
    /// If an error is encountered while writing to the store, it will be
    /// returned.
    pub fn insert(&mut self, value: T) -> Result<()> {
        self.map.insert(value, ())
    }

    /// Removes the given value from the set.
    ///
    /// If the value is not in the set, this method is a no-op. However, it will
    /// still issue a deletion to the underlying store which may have some
    /// overhead.
    ///
    /// If an error is encountered while deleting from the store, it will be
    /// returned.
    pub fn delete<U: Borrow<T>>(&mut self, value: U) -> Result<()> {
        self.map.delete(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::*;

    #[test]
    fn simple() {
        let mut store = MapStore::new();
        let mut set: Set<_, u64> = Set::wrap_store(&mut store).unwrap();

        assert_eq!(set.contains(1234).unwrap(), false);

        set.insert(1234).unwrap();
        assert_eq!(set.contains(1234).unwrap(), true);

        set.insert(1234).unwrap();

        set.delete(1234).unwrap();
        assert_eq!(set.contains(1234).unwrap(), false);
        set.delete(1234).unwrap();
    }

    #[test]
    fn read_only() {
        let mut store = MapStore::new();
        let mut set: Set<_, u64> = (&mut store).wrap().unwrap();
        set.insert(1234).unwrap();
        set.insert(5678).unwrap();

        let store = store;
        let set: Set<_, u64> = store.wrap().unwrap();
        assert_eq!(set.contains(0).unwrap(), false);
        assert_eq!(set.contains(1234).unwrap(), true);
        assert_eq!(set.contains(5678).unwrap(), true);
    }
}
