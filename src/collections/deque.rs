use super::map::{ChildMut, Map, ReadOnly, Ref};
use crate::call::Call;
use crate::client::Client;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::DefaultBackingStore;
use crate::store::{Read, Store, Write};
use crate::Result;
use serde::{Deserialize, Serialize};

#[derive(Query, Encode, Decode, Serialize, Deserialize, Describe)]
#[serde(bound = "")]
pub struct Deque<T, S: Default = DefaultBackingStore> {
    meta: Meta,
    #[serde(skip)]
    map: Map<u64, T, S>,
}

impl<T, S: Default> Deque<T, S> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: State<S>, S: Default + Read> Deque<T, S> {
    pub fn with_store(store: Store<S>) -> Result<Self> {
        Ok(Self {
            meta: Meta::default(),
            map: Map::with_store(store)?,
        })
    }
}

impl<T, S: Default> Default for Deque<T, S> {
    fn default() -> Self {
        Deque {
            meta: Meta::default(),
            map: Map::default(),
        }
    }
}

impl<T, S: Default> std::fmt::Debug for Deque<T, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deque").field("meta", &self.meta).finish()
    }
}

#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize)]
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

impl<T, S: Default> From<Deque<T, S>> for Meta {
    fn from(deque: Deque<T, S>) -> Meta {
        deque.meta
    }
}

impl<T: Call + State<S>, S: Write + Default> Call for Deque<T, S> {
    type Call = (u64, T::Call);

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let (index, subcall) = call;
        self.get_mut(index)?.call(subcall)
    }
}

// TODO: use derive(State) once it supports generic parameters
impl<T: State<S>, S: Read + Default> State<S> for Deque<T, S> {
    fn attach(&mut self, store: Store<S>) -> Result<()>
    where
        S: Read,
    {
        self.map.attach(store)
    }

    fn flush(&mut self) -> Result<()>
    where
        S: Write,
    {
        self.map.flush()
    }
}

impl<T: State<S>, S: Read + Default> Deque<T, S> {
    #[query]
    pub fn len(&self) -> u64 {
        self.meta.tail - self.meta.head
    }

    #[query]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[query]
    pub fn get(&self, index: u64) -> Result<Option<Ref<T>>> {
        self.map.get(index + self.meta.head)
    }

    #[query]
    pub fn front(&self) -> Result<Option<Ref<T>>> {
        self.map.get(self.meta.head)
    }

    #[query]
    pub fn back(&self) -> Result<Option<Ref<T>>> {
        self.map.get(self.meta.tail - 1)
    }
}

impl<T: State<S>, S: Write + Default> Deque<T, S> {
    pub fn get_mut(&mut self, index: u64) -> Result<Option<ChildMut<u64, T, S>>> {
        self.map.get_mut(index + self.meta.head)
    }

    pub fn push_back(&mut self, value: T) -> Result<()> {
        let index = self.meta.tail;
        self.meta.tail += 1;
        self.map.insert(index, value)?;
        Ok(())
    }

    pub fn push_front(&mut self, value: T) -> Result<()> {
        self.meta.head -= 1;
        let index = self.meta.head;
        self.map.insert(index, value)?;
        Ok(())
    }

    pub fn pop_front(&mut self) -> Result<Option<ReadOnly<T>>> {
        if self.is_empty() {
            return Ok(None);
        }

        self.meta.head += 1;
        self.map.remove(self.meta.head - 1)
    }

    pub fn pop_back(&mut self) -> Result<Option<ReadOnly<T>>> {
        if self.is_empty() {
            return Ok(None);
        }

        self.meta.tail -= 1;
        self.map.remove(self.meta.tail)
    }

    pub fn front_mut(&mut self) -> Result<Option<ChildMut<u64, T, S>>> {
        self.get_mut(0)
    }

    pub fn back_mut(&mut self) -> Result<Option<ChildMut<u64, T, S>>> {
        self.get_mut(self.len() - 1)
    }
}

// TODO: use derive(Client)
impl<T, U: Clone + Send, S: Default> Client<U> for Deque<T, S> {
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
    fn deque_u32_push_front() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(42).unwrap();
        assert_eq!(deque.len(), 1);
    }

    #[test]
    fn deque_u32_push_back() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();
        assert_eq!(deque.len(), 1);
    }

    #[test]
    fn deque_u32_pop_front_empty() {
        let mut deque: Deque<u32> = Deque::new();

        assert!(deque.pop_front().unwrap().is_none());
    }

    #[test]
    fn deque_u32_pop_front() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(42).unwrap();
        assert_eq!(*deque.pop_front().unwrap().unwrap(), 42);
        assert!(deque.is_empty());
    }

    #[test]
    fn deque_u32_pop_back_empty() {
        let mut deque: Deque<u32> = Deque::new();

        assert!(deque.pop_back().unwrap().is_none());
    }

    #[test]
    fn deque_u32_pop_back() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();
        assert_eq!(*deque.pop_back().unwrap().unwrap(), 42);
        assert!(deque.is_empty());
    }

    #[test]
    fn deque_u32_get() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(12).unwrap();
        deque.push_back(13).unwrap();
        deque.push_front(1).unwrap();

        assert_eq!(*deque.get(0).unwrap().unwrap(), 1);
        assert_eq!(*deque.get(1).unwrap().unwrap(), 12);
        assert_eq!(*deque.get(2).unwrap().unwrap(), 13);
    }

    #[test]
    fn deque_u32_get_iob() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(12).unwrap();
        deque.push_back(13).unwrap();
        deque.push_front(1).unwrap();

        assert!(deque.get(3).unwrap().is_none());
    }

    #[test]
    fn deque_u32_front() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(42).unwrap();
        assert_eq!(*deque.front().unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_u32_back() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();
        assert_eq!(*deque.back().unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_u32_front_back() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();

        assert_eq!(
            *deque.front().unwrap().unwrap(),
            *deque.back().unwrap().unwrap()
        )
    }

    #[test]
    fn deque_u32_get_mut() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(42).unwrap();
        assert_eq!(*deque.get_mut(0).unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_complex_types() {
        let mut deque: Deque<Map<u32, u32>> = Deque::new();

        deque.push_front(Map::new()).unwrap();

        let map = deque.pop_front().unwrap().unwrap();

        assert!(map.get(1).unwrap().is_none());
    }
}
