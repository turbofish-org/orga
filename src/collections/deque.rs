//! A double-ended queue backed by a store
use serde::Serialize;

use super::map::{ChildMut, Map, ReadOnly, Ref};
use crate::call::Call;
use crate::collections::map::Iter as MapIter;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::Migrate;
use crate::orga;
use crate::query::FieldQuery;
use crate::state::State;
use crate::store::Store;
use crate::Result;

/// A double-ended queue implementation backed by a [Map].
///
/// `Deque` provides efficient insertion and deletion at both ends, and
/// efficient iteration in either direction.
#[derive(FieldQuery, Encode, Decode)]
pub struct Deque<T> {
    meta: Meta,
    map: Map<u64, T>,
}

impl<T> Describe for Deque<T>
where
    T: State + Describe,
{
    fn describe() -> crate::describe::Descriptor {
        use crate::describe::Builder;
        Builder::new::<Self>()
            .dynamic_child::<u64, T>(|mut query_bytes| {
                query_bytes.extend_from_slice(&[129]);
                query_bytes
            })
            .build()
    }
}

impl<T> Deque<T> {
    /// Create a new empty deque.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: State> Deque<T> {
    /// Create a new deque with the provided backing store.
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

impl<T: Serialize + State> Serialize for Deque<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeSeq};
        let mut seq = serializer.serialize_seq(None)?;
        for entry in self.iter().map_err(Error::custom)? {
            let value = entry.map_err(Error::custom)?;
            seq.serialize_element(&*value)?;
        }
        seq.end()
    }
}

/// The head and tail indices of a [Deque].
///
/// The `head` and `tail` represent the integer indices of the first and last
/// elements in the deque, respectively.
///
/// - Push front: decrements the head index
/// - Push back: increments the tail index
/// - Pop front: increments the head index
/// - Pop back: decrements the tail index
///
/// By default, the head and tail are set to the midpoint of the u64 range,
/// `u64::MAX / 2`.
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
        self.meta.flush(out)?;
        self.map.flush(out)
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let mut value = Self {
            meta: Meta::load(store.clone(), bytes)?,
            map: Map::load(store.clone(), bytes)?,
        };

        value.attach(store)?;

        Ok(value)
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

#[orga]
impl<T: State> Deque<T> {
    #[query]
    pub fn len(&self) -> u64 {
        self.meta.tail - self.meta.head
    }

    #[query]
    pub fn get_raw(&self, key: u64) -> Result<Option<Ref<T>>> {
        self.map.get(key)
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

    /// Retain elements in the deque that satisfy a predicate, removing elements
    /// which do not.
    ///
    /// The order of elements is preserved.
    pub fn retain<F>(&mut self, mut f: F) -> Result<()>
    where
        F: FnMut(ChildMut<u64, T>) -> Result<bool>,
    {
        let len = self.len();
        let mut retained_index = 0;
        let mut curr_index = 0;

        while curr_index < len {
            // Unwrapping in this situation is safe because the index is always in bounds
            // and given that there is no remove by index operation on Deque and pop_front
            // and pop_back update the head and tail indices respectively, there will be
            // no removed, None values in the Deque between flushes
            if !f(self.get_mut(curr_index)?.unwrap())? {
                curr_index += 1;
                break;
            }
            curr_index += 1;
            retained_index += 1;
        }

        while curr_index < len {
            if !f(self.get_mut(curr_index)?.unwrap())? {
                curr_index += 1;
                continue;
            }

            self.swap(retained_index, curr_index)?;
            curr_index += 1;
            retained_index += 1;
        }

        if curr_index != retained_index {
            while self.len() > retained_index {
                self.pop_back()?;
            }
        }

        Ok(())
    }

    /// Retain elements in the deque that satisfy a predicate, removing elements
    /// which do not.
    ///
    /// Unlike [Deque::retain], this method does not necessarily preserve the
    /// order of elements, but may be faster.
    pub fn retain_unordered<F>(&mut self, mut f: F) -> Result<()>
    where
        F: FnMut(ChildMut<u64, T>) -> Result<bool>,
    {
        let mut i = 0;
        let mut len = self.len();
        while i < len {
            let item = self.get_mut(i)?.unwrap();
            if !f(item)? {
                self.swap_remove_back(i)?;
                len -= 1;
                continue;
            }
            i += 1;
        }
        Ok(())
    }
}

impl<'a, T: State> Deque<T> {
    /// Create an iterator over the elements of the deque.
    pub fn iter(&'a self) -> Result<Iter<'a, T>> {
        Ok(Iter {
            map_iter: self.map.iter()?,
        })
    }
}

impl<T: State> Deque<T> {
    /// Returns a mutable reference to the element at the given index, or `None`
    /// if the index is out of bounds.
    pub fn get_mut(&mut self, index: u64) -> Result<Option<ChildMut<u64, T>>> {
        self.map.get_mut(index + self.meta.head)
    }

    /// Push a value onto the back of the deque.
    pub fn push_back(&mut self, value: T) -> Result<()> {
        let index = self.meta.tail;
        self.meta.tail += 1;
        self.map.insert(index, value)?;
        Ok(())
    }

    /// Push a value onto the front of the deque.
    pub fn push_front(&mut self, value: T) -> Result<()> {
        self.meta.head -= 1;
        let index = self.meta.head;
        self.map.insert(index, value)?;
        Ok(())
    }

    /// Remove and return the element at the front of the deque, or `None` if
    /// the deque is empty.
    pub fn pop_front(&mut self) -> Result<Option<ReadOnly<T>>> {
        if self.is_empty() {
            return Ok(None);
        }

        self.meta.head += 1;
        self.map.remove(self.meta.head - 1)
    }

    /// Remove and return the element at the back of the deque, or `None` if
    /// the deque is empty.
    pub fn pop_back(&mut self) -> Result<Option<ReadOnly<T>>> {
        if self.is_empty() {
            return Ok(None);
        }

        self.meta.tail -= 1;
        self.map.remove(self.meta.tail)
    }

    /// Returns a mutable reference to the element at the front of the deque, or
    /// `None` if the deque is empty.
    pub fn front_mut(&mut self) -> Result<Option<ChildMut<u64, T>>> {
        self.get_mut(0)
    }

    /// Returns a mutable reference to the element at the back of the deque, or
    /// `None` if the deque is empty.
    pub fn back_mut(&mut self) -> Result<Option<ChildMut<u64, T>>> {
        self.get_mut(self.len() - 1)
    }

    /// Swap two elements in the deque by their indices.
    pub fn swap(&mut self, i: u64, j: u64) -> Result<()> {
        let i = i + self.meta.head;
        let j = j + self.meta.head;
        self.map.swap(i, j)
    }

    /// Swap the element at the given index with the front of the deque and pop
    /// it from the front.
    pub fn swap_remove_front(&mut self, i: u64) -> Result<()> {
        self.swap(i, 0)?;
        self.pop_front()?;
        Ok(())
    }

    /// Swap the element at the given index with the back of the deque and pop
    /// it from the back.
    pub fn swap_remove_back(&mut self, i: u64) -> Result<()> {
        self.swap(i, self.len() - 1)?;
        self.pop_back()?;
        Ok(())
    }
}

impl<T: Migrate> Migrate for Deque<T> {
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            meta: Meta::migrate(Store::default(), Store::default(), bytes)?,
            map: Map::migrate(src, dest, bytes)?,
        })
    }
}

/// An iterator over the elements of a [Deque], backed by an underlying
/// [MapIter]. Supports both forward and reverse iteration.
pub struct Iter<'a, T>
where
    T: State,
{
    map_iter: MapIter<'a, u64, T>,
}

impl<'a, T> Iterator for Iter<'a, T>
where
    T: State,
{
    type Item = Result<Ref<'a, T>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.map_iter.next().map(|entry| match entry {
            Ok(entry) => Ok(entry.1),
            Err(err) => Err(err),
        })
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T>
where
    T: State,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.map_iter.next_back().map(|entry| match entry {
            Ok(entry) => Ok(entry.1),
            Err(err) => Err(err),
        })
    }
}

#[allow(unused_imports)]
mod test {
    use super::{Deque, Map, Meta};
    use crate::state::State;
    use crate::store::MapStore;
    use crate::store::Store;

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

    #[test]
    fn deque_u32_iter() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(42).unwrap();
        deque.push_back(43).unwrap();
        deque.push_front(1).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert_eq!(*iter.next().unwrap().unwrap(), 42);
        assert_eq!(*iter.next().unwrap().unwrap(), 43);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_iter_rev() -> crate::Result<()> {
        let mut store = Store::with_map_store().sub(&[123]);
        let mut deque: Deque<u32> = Deque::new();
        deque.attach(store.clone())?;

        deque.push_back(1)?;
        deque.push_back(2)?;
        deque.push_back(3)?;

        let mut bytes = vec![];

        use crate::store::Write;
        deque.flush(&mut bytes)?;
        store.put(vec![], bytes.clone()).unwrap();

        let mut deque: Deque<u32> = Deque::load(store.clone(), &mut &bytes[..])?;
        deque.attach(store)?;

        let mut iter = deque.iter()?.rev();
        assert_eq!(*iter.next().unwrap()?, 3);
        assert_eq!(*iter.next().unwrap()?, 2);
        assert_eq!(*iter.next().unwrap()?, 1);
        assert!(iter.next().is_none());

        Ok(())
    }

    #[test]
    fn deque_iter_rev_noflush() -> crate::Result<()> {
        let store = Store::with_map_store().sub(&[123]);
        let mut deque: Deque<u32> = Deque::new();
        deque.attach(store.clone())?;

        deque.push_back(1)?;
        deque.push_back(2)?;
        deque.push_back(3)?;

        let mut iter = deque.iter()?.rev();
        assert_eq!(*iter.next().unwrap()?, 3);
        assert_eq!(*iter.next().unwrap()?, 2);
        assert_eq!(*iter.next().unwrap()?, 1);
        assert!(iter.next().is_none());

        Ok(())
    }

    #[test]
    fn deque_swap() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_front(42).unwrap();
        deque.push_back(43).unwrap();
        deque.push_front(1).unwrap();

        deque.swap(0, 1).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 42);
        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert_eq!(*iter.next().unwrap().unwrap(), 43);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_swap_remove_front() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();
        deque.push_back(43).unwrap();
        deque.push_back(1).unwrap();

        deque.swap_remove_front(1).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 42);
        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_swap_remove_back() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();
        deque.push_back(43).unwrap();
        deque.push_back(1).unwrap();

        deque.swap_remove_back(0).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert_eq!(*iter.next().unwrap().unwrap(), 43);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(42).unwrap();
        deque.push_back(43).unwrap();
        deque.push_back(1).unwrap();

        deque.retain(|x| Ok(*x != 43)).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 42);
        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain_order_preservation() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();
        deque.push_back(4).unwrap();
        deque.push_back(5).unwrap();
        deque.push_back(6).unwrap();

        deque.retain(|x| Ok(*x != 3 && *x != 4)).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert_eq!(*iter.next().unwrap().unwrap(), 2);
        assert_eq!(*iter.next().unwrap().unwrap(), 5);
        assert_eq!(*iter.next().unwrap().unwrap(), 6);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retained_with_popped_element() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();
        deque.push_back(4).unwrap();
        deque.push_back(5).unwrap();
        deque.push_back(6).unwrap();

        deque.pop_front().unwrap().unwrap();

        deque.retain(|x| Ok(*x != 1)).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 2);
        assert_eq!(*iter.next().unwrap().unwrap(), 3);
        assert_eq!(*iter.next().unwrap().unwrap(), 4);
        assert_eq!(*iter.next().unwrap().unwrap(), 5);
        assert_eq!(*iter.next().unwrap().unwrap(), 6);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain_empty() {
        let mut deque: Deque<u32> = Deque::new();
        deque.retain(|_| Ok(true)).unwrap();

        let mut iter = deque.iter().unwrap();
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain_unordered() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();
        deque.push_back(4).unwrap();
        deque.push_back(5).unwrap();
        deque.push_back(6).unwrap();

        deque.retain_unordered(|x| Ok(*x != 3 && *x != 4)).unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert_eq!(*iter.next().unwrap().unwrap(), 2);
        assert_eq!(*iter.next().unwrap().unwrap(), 6);
        assert_eq!(*iter.next().unwrap().unwrap(), 5);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain_unordered_overlap() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();
        deque.push_back(4).unwrap();
        deque.push_back(5).unwrap();
        deque.push_back(6).unwrap();

        deque
            .retain_unordered(|x| Ok(*x != 3 && *x != 4 && *x != 6))
            .unwrap();

        let mut iter = deque.iter().unwrap();

        assert_eq!(*iter.next().unwrap().unwrap(), 1);
        assert_eq!(*iter.next().unwrap().unwrap(), 2);
        assert_eq!(*iter.next().unwrap().unwrap(), 5);
        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain_unordered_single() {
        let mut deque: Deque<u32> = Deque::new();

        deque.push_back(1).unwrap();

        deque.retain_unordered(|x| Ok(*x != 1)).unwrap();

        let mut iter = deque.iter().unwrap();

        assert!(iter.next().is_none());
    }

    #[test]
    fn deque_retain_unordered_none() {
        let mut deque: Deque<u32> = Deque::new();
        deque.retain_unordered(|_| Ok(true)).unwrap();

        let mut iter = deque.iter().unwrap();
        assert!(iter.next().is_none());
    }
}
