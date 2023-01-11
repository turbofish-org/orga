use super::map::{ChildMut, Map, ReadOnly, Ref};
use crate::call::Call;
use crate::client::Client;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::{MigrateFrom, MigrateInto};
use crate::orga;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::Result;

#[derive(Query, Encode, Decode)]
pub struct Deque<T> {
    meta: Meta,
    map: Map<u64, T>,
}

impl<T> Deque<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: State> Deque<T> {
    pub fn with_store(store: Store) -> Result<Self> {
        Ok(Self {
            meta: Meta::default(),
            map: Map::with_store(store)?,
        })
    }
}

impl<T> Default for Deque<T> {
    fn default() -> Self {
        Deque {
            meta: Meta::default(),
            map: Map::default(),
        }
    }
}

impl<T> std::fmt::Debug for Deque<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deque").field("meta", &self.meta).finish()
    }
}

#[orga(skip(Default))]
#[derive(Clone, Debug)]
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

impl<T> From<Deque<T>> for Meta {
    fn from(deque: Deque<T>) -> Meta {
        deque.meta
    }
}

impl<T: Call + State> Call for Deque<T> {
    type Call = (u64, T::Call);

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let (index, subcall) = call;
        self.get_mut(index)?.call(subcall)
    }
}

// TODO: use derive(State) once it supports generic parameters
impl<T: State> State for Deque<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.map.attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        self.map.flush(out)
    }

    fn load(store: Store, _bytes: &mut &[u8]) -> Result<Self> {
        let deque = Deque::default();
        deque.attach(store)?;

        Ok(deque)
    }
}

// impl<T: State + Describe + 'static> Describe for Deque<T> {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>()
//             .named_child::<Map<u64, T>>("map", &[], |v| {
//                 crate::describe::Builder::access(v, |v: Self| v.map)
//             })
//             .build()
//     }
// }

impl<T: State> Deque<T> {
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

impl<T: State> Deque<T> {
    pub fn get_mut(&mut self, index: u64) -> Result<Option<ChildMut<u64, T>>> {
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

    pub fn front_mut(&mut self) -> Result<Option<ChildMut<u64, T>>> {
        self.get_mut(0)
    }

    pub fn back_mut(&mut self) -> Result<Option<ChildMut<u64, T>>> {
        self.get_mut(self.len() - 1)
    }
}

// TODO: use derive(Client)
impl<T, U: Clone + Send> Client<U> for Deque<T> {
    type Client = ();

    fn create_client(_: U) {}
}

impl<T1, T2> MigrateFrom<Deque<T1>> for Deque<T2>
where
    T1: State,
    T2: MigrateFrom<T1> + State,
{
    fn migrate_from(other: Deque<T1>) -> Result<Self> {
        Ok(Deque {
            meta: other.meta,
            map: other.map.migrate_into()?,
        })
    }
}

#[allow(unused_imports)]
mod test {
    use super::{Deque, Map};
    use crate::store::MapStore;

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
