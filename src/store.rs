use crate::error::Result;

// TODO: Iterable trait so state machines can iterate through keys, or should this be required?

pub trait Read<K, V> {
  fn get<KR: AsRef<K>>(&self, key: KR) -> Result<V>;
}

pub trait Write<K, V> {
  fn put(&mut self, key: K, value: V) -> Result<()>;
}

pub trait Store<K, V>: Read<K, V> + Write<K, V> {}

impl<S, K, V> Store<K, V> for S
    where S: Read<K, V> + Write<K, V>
{}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::*;

    struct MapStore (BTreeMap<Vec<u8>, Vec<u8>>);

    impl MapStore {
        fn new() -> MapStore {
            MapStore(BTreeMap::default())
        }
    }

    impl Read<Vec<u8>, Vec<u8>> for MapStore {
        fn get<K: AsRef<Vec<u8>>>(&self, key: K) -> Result<Vec<u8>> {
            match self.0.get(key.as_ref()) {
                Some(value) => Ok(value.clone()),
                None => Err(format_err!("not found"))
            }
        }
    }

    impl Write<Vec<u8>, Vec<u8>> for MapStore {
        fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
            self.0.insert(key, value);
            Ok(())
        }
    }

    #[test]
    fn mapstore_satisfies_store_trait() {
        fn assert_store<S: Store<Vec<u8>, Vec<u8>>>(_: S) {}
        assert_store(MapStore::new());
    }
}


