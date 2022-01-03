use super::Adapter;
use crate::state::State;
use crate::store::Store;
use crate::Result;
use serde::{Deserialize, Serialize};
pub struct Map<K, V> {
    map: crate::collections::Map<Adapter<K>, Adapter<V>>,
}

impl<K, V> State for Map<K, V>
where
    K: Serialize + for<'de> Deserialize<'de>,
    V: Serialize + for<'de> Deserialize<'de>,
{
    type Encoding = ();

    fn create(store: Store, _data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            map: State::create(store, ())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.map.flush()?;
        Ok(())
    }
}

impl<K, V> From<Map<K, V>> for () {
    fn from(_: Map<K, V>) -> Self {}
}

impl<K, V> Map<K, V>
where
    K: Serialize + for<'de> Deserialize<'de>,
    V: Serialize + for<'de> Deserialize<'de>,
{
    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        self.map.insert(key.into(), value.into())
    }
}
