#[cfg(test)]
use mutagen::mutate;
use serde::Serialize;

use super::map::{ChildMut, Map, ReadOnly, Ref};
use crate::call::Call;
use crate::client::Client;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::DefaultBackingStore;
use crate::store::{Read, Store, Write};
use crate::Result;

#[derive(Query, Serialize)]
#[serde(bound(
    serialize = "T: Serialize + State<S>, S: Read",
    deserialize = "T: Deserialize<'de> + State",
))]
pub struct Deque<T, S = DefaultBackingStore> {
    meta: Meta,
    map: Map<u64, T, S>,
}

impl<T, S> std::fmt::Debug for Deque<T, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deque").field("meta", &self.meta).finish()
    }
}

#[derive(Encode, Decode, Clone, Debug, Serialize)]
pub struct Meta {
    head: u64,
    tail: u64,
}

impl Default for Meta {
    fn default() -> Self {
        let midpoint = u64::MAX / 2;
        Meta {
            head: midpoint,
            tail: midpoint,
        }
    }
}

impl<T, S> From<Deque<T, S>> for Meta {
    fn from(deque: Deque<T, S>) -> Meta {
        deque.meta
    }
}

impl<T: Call + State<S>, S: Write> Call for Deque<T, S> {
    type Call = (u64, T::Call);

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let (index, subcall) = call;
        self.get_mut(index)?.call(subcall)
    }
}

// TODO: use derive(State) once it supports generic parameters
impl<T: State<S>, S: Read> State<S> for Deque<T, S> {
    type Encoding = Meta;

    fn create(store: Store<S>, meta: Self::Encoding) -> Result<Self>
    where
        S: Read,
    {
        Ok(Deque {
            meta,
            map: Map::create(store, ())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding>
    where
        S: Write,
    {
        self.map.flush()?;
        Ok(self.meta)
    }
}

impl<T: State<S>, S: Read> Deque<T, S> {
    #[query]
    #[cfg_attr(test, mutate)]
    pub fn len(&self) -> u64 {
        self.meta.tail - self.meta.head
    }

    #[query]
    #[cfg_attr(test, mutate)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[query]
    #[cfg_attr(test, mutate)]
    pub fn get(&self, index: u64) -> Result<Option<Ref<T>>> {
        self.map.get(index + self.meta.head)
    }

    #[query]
    #[cfg_attr(test, mutate)]
    pub fn front(&self) -> Result<Option<Ref<T>>> {
        self.map.get(self.meta.head)
    }

    #[query]
    #[cfg_attr(test, mutate)]
    pub fn back(&self) -> Result<Option<Ref<T>>> {
        self.map.get(self.meta.tail - 1)
    }
}

impl<T: State<S>, S: Write> Deque<T, S> {
    #[cfg_attr(test, mutate)]
    pub fn get_mut(&mut self, index: u64) -> Result<Option<ChildMut<u64, T, S>>> {
        self.map.get_mut(index + self.meta.head)
    }

    #[cfg_attr(test, mutate)]
    pub fn push_back(&mut self, value: T::Encoding) -> Result<()> {
        let index = self.meta.tail;
        self.meta.tail += 1;
        self.map.insert(index, value)?;
        Ok(())
    }

    #[cfg_attr(test, mutate)]
    pub fn push_front(&mut self, value: T::Encoding) -> Result<()> {
        self.meta.head -= 1;
        let index = self.meta.head;
        self.map.insert(index, value)?;
        Ok(())
    }

    #[cfg_attr(test, mutate)]
    pub fn pop_front(&mut self) -> Result<Option<ReadOnly<T>>> {
        if self.is_empty() {
            return Ok(None);
        }

        self.meta.head += 1;
        self.map.remove(self.meta.head - 1)
    }

    #[cfg_attr(test, mutate)]
    pub fn pop_back(&mut self) -> Result<Option<ReadOnly<T>>> {
        if self.is_empty() {
            return Ok(None);
        }

        self.meta.tail -= 1;
        self.map.remove(self.meta.tail)
    }

    #[cfg_attr(test, mutate)]
    pub fn front_mut(&mut self) -> Result<Option<ChildMut<u64, T, S>>> {
        self.get_mut(0)
    }

    #[cfg_attr(test, mutate)]
    pub fn back_mut(&mut self) -> Result<Option<ChildMut<u64, T, S>>> {
        self.get_mut(self.len() - 1)
    }
}

// TODO: use derive(Client)
impl<T, U: Clone + Send, S> Client<U> for Deque<T, S> {
    type Client = ();

    fn create_client(_: U) {}
}

#[allow(unused_imports)]
mod test {
    use super::{Deque as OrgaDeque, Map as OrgaMap, *};
    use crate::store::MapStore;
    #[allow(dead_code)]
    type Deque<T> = OrgaDeque<T, MapStore>;
    #[allow(dead_code)]
    type Map<K, V> = OrgaMap<K, V, MapStore>;
    #[test]
    fn deque_u32_create() {
        let store = Store::new(MapStore::new());
        let _deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();
    }

    #[test]
    fn deque_u32_push_front() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(42).unwrap();
        assert_eq!(deque.len(), 1);
    }

    #[test]
    fn deque_u32_push_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_back(42).unwrap();
        assert_eq!(deque.len(), 1);
    }

    #[test]
    fn deque_u32_pop_front_empty() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        assert!(deque.pop_front().unwrap().is_none());
    }

    #[test]
    fn deque_u32_pop_front() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(42).unwrap();
        assert_eq!(*deque.pop_front().unwrap().unwrap(), 42);
        assert!(deque.is_empty());
    }

    #[test]
    fn deque_u32_pop_back_empty() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        assert!(deque.pop_back().unwrap().is_none());
    }

    #[test]
    fn deque_u32_pop_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_back(42).unwrap();
        assert_eq!(*deque.pop_back().unwrap().unwrap(), 42);
        assert!(deque.is_empty());
    }

    #[test]
    fn deque_u32_get() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(12).unwrap();
        deque.push_back(13).unwrap();
        deque.push_front(1).unwrap();

        assert_eq!(*deque.get(0).unwrap().unwrap(), 1);
        assert_eq!(*deque.get(1).unwrap().unwrap(), 12);
        assert_eq!(*deque.get(2).unwrap().unwrap(), 13);
    }

    #[test]
    fn deque_u32_get_iob() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(12).unwrap();
        deque.push_back(13).unwrap();
        deque.push_front(1).unwrap();

        assert!(deque.get(3).unwrap().is_none());
    }

    #[test]
    fn deque_u32_front() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(42).unwrap();
        assert_eq!(*deque.front().unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_u32_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_back(42).unwrap();
        assert_eq!(*deque.back().unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_u32_front_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_back(42).unwrap();

        assert_eq!(
            *deque.front().unwrap().unwrap(),
            *deque.back().unwrap().unwrap()
        )
    }

    #[test]
    fn deque_u32_get_mut() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(42).unwrap();
        assert_eq!(*deque.get_mut(0).unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_complex_types() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<Map<u32, u32>> = Deque::create(store, Meta::default()).unwrap();

        deque.push_front(()).unwrap();

        let map = deque.pop_front().unwrap().unwrap();

        assert!(map.get(1).unwrap().is_none());
    }
}
