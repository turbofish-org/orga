use std::borrow::Borrow;
use crate::{State, Store, Encode, Decode, Result};
use super::Map;

pub struct Set<S: Store, T: Encode + Decode> {
    map: Map<S, T, ()>
}

impl<S: Store, T: Encode + Decode> State<S> for Set<S, T> {
    fn wrap_store(store: S) -> Result<Self> {
        Ok(Self {
            map: Map::wrap_store(store)?
        })
    }
}

impl<S: Store, T: Encode + Decode> Set<S, T> {
    pub fn insert(&mut self, value: T) -> Result<()> {
        self.map.insert(value, ())
    }

    pub fn delete<U: Borrow<T>>(&mut self, value: U) -> Result<()> {
        self.map.delete(value)
    }

    pub fn contains<U: Borrow<T>>(&self, value: U) -> Result<bool> {
        Ok(self.map.get(value)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

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
}
