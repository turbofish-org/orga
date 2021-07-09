use super::map::{Map, Child, ChildMut};
use crate::state::State;
use crate::store::DefaultBackingStore;
use crate::store::{Read, Store, Write};
use crate::Result;
use crate::encoding::{Encode, Decode};

pub struct Deque<T, S = DefaultBackingStore> {
    meta: Meta,
    map: Map<u64, T, S>,
}

#[derive(Encode, Decode)]
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
    pub fn len(&self) -> u64 {
        self.meta.tail - self.meta.head
    }

    pub fn get(&self, index: u64) -> Result<Option<Child<T>>> {
        self.map.get(index + self.meta.head)
    }

    pub fn front(&self) -> Result<Option<Child<T>>> {
        self.map.get(self.meta.head)
    }

    pub fn back(&self) -> Result<Option<Child<T>>> {
        self.map.get(self.meta.tail - 1)
    }
}

impl<T: State<S>, S: Write> Deque<T, S> {
    pub fn push_back(&mut self, value: T::Encoding) -> Result<()> {
        let index = self.meta.tail;
        self.meta.tail += 1;
        // TODO: use insert
        self.map.entry(index)?.or_insert(value)?;
        Ok(())
    }

    pub fn push_front(&mut self, value: T::Encoding) -> Result<()> {
        self.meta.head -= 1;
        let index = self.meta.head;
        // TODO: use insert
        self.map.entry(index)?.or_insert(value)?;
        Ok(())
    }

    pub fn pop_front(&mut self) -> Result<Option<T::Encoding>> {
        if self.len() == 0 {
            return Ok(None);
        }

        self.meta.head += 1;
        self.map.remove(self.meta.head - 1)
    }

    pub fn pop_back(&mut self) -> Result<()> {
        if self.len() == 0 {
            return Ok(());
        }

        self.meta.tail -= 1;
        self.map.remove(self.meta.tail);
        
        Ok(())
    }

    pub fn get_mut(&mut self, index: u64) -> Result<Option<ChildMut<u64, T, S>>> {
        self.map.get_mut(index + self.meta.head)
    }
}

mod test {
    use super::*;
    use crate::store::MapStore;

    #[test]
    fn deque_u32_create() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();
    }

    #[test]
    fn deque_u32_push_front() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_front(42).unwrap();
    }

    #[test]
    fn deque_u32_push_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_back(42).unwrap();
    }

    #[test]
    fn deque_u32_pop_front_empty() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        assert_eq!(deque.pop_front().unwrap(), ());
    }

    #[test]
    fn deque_u32_pop_front() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();
        
        deque.push_front(42);
        assert_eq!(deque.pop_front().unwrap(), ());
    }

    #[test]
    fn deque_u32_pop_back_empty() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        assert_eq!(deque.pop_back().unwrap(), ())
    }

    #[test]
    fn deque_u32_pop_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_back(42);
        assert_eq!(deque.pop_back().unwrap(), ())
    }

    #[test]
    fn deque_u32_get() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_front(12);
        deque.push_back(13);
        deque.push_front(1);

        assert_eq!(*deque.get(0).unwrap().unwrap(), 1);
        assert_eq!(*deque.get(1).unwrap().unwrap(), 12);
        assert_eq!(*deque.get(2).unwrap().unwrap(), 13);
    }

    #[test]
    fn deque_u32_get_iob() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_front(12);
        deque.push_back(13);
        deque.push_front(1);

        let result = match deque.get(3).unwrap() {
            Some(_) => assert!(false),
            None => ()
        };
    }

    #[test]
    fn deque_u32_front() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_front(42);
        assert_eq!(*deque.front().unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_u32_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_back(42);
        assert_eq!(*deque.back().unwrap().unwrap(), 42)
    }

    #[test]
    fn deque_u32_front_back() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_back(42);
        assert_eq!(*deque.front().unwrap().unwrap(), *deque.back().unwrap().unwrap())
    }

    #[test]
    fn deque_u32_get_mut() {
        let store = Store::new(MapStore::new());
        let mut deque: Deque<u32> = Deque::create(store.clone(), Meta::default()).unwrap();

        deque.push_front(42);
        assert_eq!(*deque.get_mut(0).unwrap().unwrap(), 42)
    }
}
