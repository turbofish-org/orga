use std::marker::PhantomData;

use failure::bail;

use crate::encoding::{Decode, Encode};
use crate::state::{State, Value, Query};
use crate::store::{Read, Store};
use crate::Result;

/// A double-ended queue data structure.
pub struct Deque<S: Read, T: Encode + Decode> {
    store: S,
    state: Meta,
    item_type: PhantomData<T>,
}

/// The index containing the head and tail indices of the deque.
///
/// This lets us pop elements from the front of the deque without needing to
/// update the keys of all remaining elements (an O(N) operation) - we can
/// simply delete the first element then increment `head`.
///
/// A deque will have exactly one `Meta` value, stored at the empty key, which
/// will be read when `Deque::wrap_store` is called.
#[derive(Debug, Default, Encode, Decode)]
struct Meta {
    head: u64,
    tail: u64,
}

impl<S: Read, T: Encode + Decode> State<S> for Deque<S, T> {
    /// Wraps the store to allow reading from and writing to the deque.
    ///
    /// This `wrap_store` implementation will also read the `Meta` value
    /// (containing the head and tail indices) and store its contents in memory.
    fn wrap_store(store: S) -> Result<Self> {
        let state: Meta = Value::wrap_store(&store)?.get_or_default()?;

        Ok(Self {
            store,
            state,
            item_type: PhantomData,
        })
    }
}

impl<S: Read, T: Encode + Decode> Deque<S, T> {
    /// Gets the length of the deque.
    ///
    /// This method does not read from the state since the head and tail indices
    /// are kept in memory, so it is a cheap operation.
    pub fn len(&self) -> u64 {
        self.state.tail - self.state.head
    }

    /// Gets the element with the given index.
    ///
    /// If the index is out of bounds, this method will panic. If getting from
    /// the store or decoding the store value returns an error, this method will
    /// return the error.
    pub fn get(&self, index: u64) -> Result<T> {
        self.get_fixed(self.fixed_index(index))
    }

    /// Gets the element with the given index, or None if the index is out of
    /// bounds.
    ///
    /// If getting from the store or decoding the store value returns an error,
    /// this method will return the error.
    pub fn maybe_get(&self, index: u64) -> Result<Option<T>> {
        Ok(if self.is_oob(index) {
            None
        } else {
            Some(self.get(index)?)
        })
    }

    /// Calculates the current fixed index of the given index.
    ///
    /// An element's fixed index remains constant even as elements are pushed or
    /// popped from the front of the deque. This is useful when storing data
    /// which references other elements of the deque, using the index as a
    /// pseudo-pointer.
    ///
    /// For example, if we have a deque with 2 elements, "foo" and "bar", index
    /// 0 will be "foo" and index 1 will be "bar". But if we pop "foo", now
    /// index 0 will be "bar", but its fixed index will still be 1.
    pub fn fixed_index(&self, index: u64) -> u64 {
        if self.is_oob(index) {
            panic!("Index out of bounds");
        }

        index + self.state.head
    }

    /// Gets an element by its fixed index.
    ///
    /// See [#method.fixed_index] for more info about fixed indices.
    pub fn get_fixed(&self, index: u64) -> Result<T> {
        if index < self.state.head || index >= self.state.tail {
            bail!("Index out of bounds");
        }
        let bytes = self.store.get(&store_key(index)[..])?;
        T::decode(bytes.unwrap().as_slice())
    }

    /// Returns true if the index is out of bounds, or false if the index is
    /// valid.
    pub fn is_oob(&self, index: u64) -> bool {
        index >= self.len()
    }

    /// Creates an iterator over the deque's elements.
    ///
    /// Iteration happens in order by index.
    pub fn iter<'a>(&'a self) -> Iter<'a, S, T> {
        Iter {
            deque: self,
            index: 0,
        }
    }

    /// Returns true if the deque has length 0, false otherwise.
    ///
    /// This method does not read from the state since the head and tail indices
    /// are kept in memory, so it is a cheap operation.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gets the last element of the deque, or `None` if the deque is empty.
    pub fn back(&self) -> Result<Option<T>> {
        if self.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.get(self.len() - 1)?))
        }
    }
}

impl<S: Read, T: Encode + Decode> Query for Deque<S, T> {
    type Request = u64;
    type Response = Option<T>;

    fn query(&self, index: u64) -> Result<Option<T>> {
        self.maybe_get(index)
    }

    fn resolve(&self, index: u64) -> Result<()> {
        if !self.is_oob(index) {
            self.store.get(&store_key(index)[..])?;
        }

        Ok(())
    }
}

impl<S: Store, T: Encode + Decode> Deque<S, T> {
    /// Appends an element to the back of the deque.
    pub fn push_back(&mut self, value: T) -> Result<()> {
        let index = self.state.tail;

        self.state.tail += 1;
        Value::<_, Meta>::wrap_store(&mut self.store)?.set(&self.state)?;

        let bytes = value.encode()?;
        self.store.put(store_key(index).to_vec(), bytes)
    }

    /// Pops an element off the front of the deque and deletes it, then returns
    /// the popped value.
    pub fn pop_front(&mut self) -> Result<Option<T>> {
        if self.is_empty() {
            return Ok(None);
        }

        let value = self.get(0)?;

        self.state.head += 1;
        Value::<_, Meta>::wrap_store(&mut self.store)?.set(&self.state)?;

        Ok(Some(value))
    }

    /// Replaces the element at the given index.
    pub fn set(&mut self, index: u64, value: T) -> Result<()> {
        let index = self.fixed_index(index);

        let bytes = value.encode()?;
        self.store.put(store_key(index).to_vec(), bytes)
    }

    /// Removes all elements.
    ///
    /// This operation will be expensive for large deques, since each element is
    /// individually deleted.
    pub fn clear(&mut self) -> Result<()> {
        while !self.is_empty() {
            self.pop_front()?;
        }
        Ok(())
    }

    /// Removes all elements and appends them to the given deque in order.
    ///
    /// This operation will be expensive for large deques, since each element is
    /// individually deleted then pushed.
    pub fn drain_into<S2: Store>(&mut self, other: &mut Deque<S2, T>) -> Result<()> {
        loop {
            match self.pop_front()? {
                None => return Ok(()),
                Some(value) => other.push_back(value)?,
            };
        }
    }
}

/// Derives the key for the given index.
fn store_key(index: u64) -> [u8; 8] {
    // TODO: 0 should be (u64::MAX / 2) so elements can be prepended
    index.to_be_bytes()
}

/// An iterator over the entries of a `Deque`.
pub struct Iter<'a, S: Read, T: Encode + Decode> {
    deque: &'a Deque<S, T>,
    index: u64,
}

impl<'a, S: Read, T: Encode + Decode> Iterator for Iter<'a, S, T> {
    type Item = Result<T>;

    /// Gets the next value from the iterator, or `None` if the iterator is now
    /// out of bounds.
    fn next(&mut self) -> Option<Result<T>> {
        if self.index >= self.deque.len() {
            return None;
        }
        Some(self.next_unchecked())
    }
}

impl<'a, S: Read, T: Encode + Decode> Iter<'a, S, T> {
    /// Attempts to get the value for the iterator's current index then
    /// increments the index.
    ///
    /// If the index is out of bounds, the method will panic.
    fn next_unchecked(&mut self) -> Result<T> {
        let value = self.deque.get(self.index)?;
        // TODO: invalidate iterator after first Err?
        self.index += 1;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{store::*, *};

    #[test]
    fn simple() {
        let mut store = MapStore::new();

        let mut deque: Deque<_, u64> = Deque::wrap_store(&mut store).unwrap();
        assert_eq!(deque.len(), 0);

        deque.push_back(10).unwrap();
        assert_eq!(deque.len(), 1);
        assert_eq!(deque.get(0).unwrap(), 10);

        deque.push_back(20).unwrap();
        assert_eq!(deque.len(), 2);
        assert_eq!(deque.get(0).unwrap(), 10);
        assert_eq!(deque.get(1).unwrap(), 20);
    }

    #[test]
    fn fixed() {
        let mut store = MapStore::new();

        let mut deque: Deque<_, u64> = Deque::wrap_store(&mut store).unwrap();
        assert_eq!(deque.len(), 0);

        deque.push_back(10).unwrap();
        deque.push_back(20).unwrap();
        assert_eq!(deque.len(), 2);
        assert_eq!(deque.get_fixed(0).unwrap(), 10);
        assert_eq!(deque.fixed_index(0), 0);
        assert_eq!(deque.get_fixed(1).unwrap(), 20);
        assert_eq!(deque.fixed_index(1), 1);

        deque.pop_front().unwrap();
        assert_eq!(deque.len(), 1);
        assert_eq!(deque.get_fixed(1).unwrap(), 20);
        assert_eq!(deque.fixed_index(0), 1);
    }

    #[test]
    fn reinstantiate() {
        let mut store = MapStore::new();

        let mut deque: Deque<_, u64> = Deque::wrap_store(&mut store).unwrap();
        assert_eq!(deque.len(), 0);

        deque.push_back(1).unwrap();
        assert_eq!(deque.len(), 1);
        assert_eq!(deque.get(0).unwrap(), 1);

        let mut deque: Deque<_, u64> = Deque::wrap_store(&mut store).unwrap();
        assert_eq!(deque.len(), 1);
        assert_eq!(deque.get(0).unwrap(), 1);
        deque.push_back(2).unwrap();
        assert_eq!(deque.len(), 2);
        assert_eq!(deque.get(0).unwrap(), 1);
        assert_eq!(deque.get(1).unwrap(), 2);
    }

    #[test]
    fn iter() {
        let mut store = MapStore::new();
        let mut deque: Deque<_, u64> = Deque::wrap_store(&mut store).unwrap();

        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();
        deque.push_back(3).unwrap();

        let collected = deque.iter().collect::<Result<Vec<u64>>>().unwrap();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn read_only() {
        let mut store = MapStore::new();
        let mut deque: Deque<_, u64> = (&mut store).wrap().unwrap();
        deque.push_back(10).unwrap();
        deque.push_back(20).unwrap();

        let store = store;
        let deque: Deque<_, u64> = store.wrap().unwrap();
        assert_eq!(deque.len(), 2);
        assert_eq!(deque.get(0).unwrap(), 10);
        assert_eq!(deque.get(1).unwrap(), 20);

        let collected = deque.iter().collect::<Result<Vec<u64>>>().unwrap();
        assert_eq!(collected, vec![10, 20]);
    }

    #[test]
    fn query_resolve_oob() {
        let mut store = RWLog::wrap(MapStore::new());
        let mut deque: Deque<_, u64> = (&mut store).wrap().unwrap();
        deque.push_back(10).unwrap();
        deque.push_back(20).unwrap();

        deque.resolve(2).unwrap();

        let (reads, _, _) = store.finish();
        assert_eq!(reads.len(), 1);
        assert!(reads.contains(&[][..]));
    }

    #[test]
    fn query_resolve_ok() {
        let mut store = RWLog::wrap(MapStore::new());
        let mut deque: Deque<_, u64> = (&mut store).wrap().unwrap();
        deque.push_back(10).unwrap();
        deque.push_back(20).unwrap();

        deque.resolve(1).unwrap();

        let (reads, _, _) = store.finish();
        assert_eq!(reads.len(), 2);
        assert!(reads.contains(&[][..]));
        assert!(reads.contains(&[0, 0, 0, 0, 0, 0, 0, 1][..]));
    }

    #[test]
    fn query_oob() {
        let mut store = MapStore::new();
        let mut deque: Deque<_, u64> = (&mut store).wrap().unwrap();
        deque.push_back(10).unwrap();
        deque.push_back(20).unwrap();
        assert_eq!(deque.query(2).unwrap(), None);
    }

    #[test]
    fn query_ok() {
        let mut store = MapStore::new();
        let mut deque: Deque<_, u64> = (&mut store).wrap().unwrap();
        deque.push_back(10).unwrap();
        deque.push_back(20).unwrap();
        assert_eq!(deque.query(1).unwrap(), Some(20));
    }
}
