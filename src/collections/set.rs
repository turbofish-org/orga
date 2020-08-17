use std::borrow::Borrow;

use crate::Result;
use crate::encoding::{Encode, Decode};
use crate::state::State;
use crate::store::{Read, Store};
use super::Map;

/// A set data structure.
pub struct Set<S: Read, T: Encode + Decode> {
    map: Map<S, T, ()>
}

impl<S: Read, T: Encode + Decode> State<S> for Set<S, T> {
    fn wrap_store(store: S) -> Result<Self> {
        Ok(Self {
            map: Map::wrap_store(store)?
        })
    }
}

impl<S: Read, T: Encode + Decode> Set<S, T> {
    pub fn contains<U: Borrow<T>>(&self, value: U) -> Result<bool> {
        Ok(self.map.get(value)?.is_some())
    }
}

impl<S: Store, T: Encode + Decode> Set<S, T> {
    pub fn insert(&mut self, value: T) -> Result<()> {
        self.map.insert(value, ())
    }

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
        let mut set: Set<_, u64> =
            Set::wrap_store(&mut store).unwrap();

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
