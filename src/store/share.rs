//! Shared references to stores.
use super::{Read, Write, KV};
use crate::Result;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

// TODO: we can probably use UnsafeCell instead of RefCell since operations are
// guaranteed not to interfere with each other (the borrows only last until the
// end of each call).

/// A shared reference to a store, allowing the store to be cloned and read from
/// or written to by multiple consumers.
///
/// `Shared` has the `Clone` trait - it is safe to clone references to the store
/// since `get`, `get_next`, `put`, and `delete` all operate atomically so there
/// will never be more than one reference borrowing the underlying store at a
/// time.
#[derive(Default)]
pub struct Shared<T>(Arc<RwLock<T>>);

impl<T> Shared<T> {
    /// Constructs a `Shared` by wrapping the given store.
    #[inline]
    pub fn new(inner: T) -> Self {
        Shared(Arc::new(RwLock::new(inner)))
    }

    /// Consumes the `Shared` and returns the inner store.
    ///
    /// # Panics
    ///
    /// Panics if the store is currently borrowed.
    pub fn into_inner(self) -> T {
        match Arc::try_unwrap(self.0) {
            Ok(inner) => inner.into_inner().unwrap(),
            _ => panic!("Store is already borrowed"),
        }
    }

    /// Returns a mutable reference to the inner store.
    ///
    /// # Panics
    ///
    /// Panics if the store is currently borrowed.
    pub fn borrow_mut(&mut self) -> RwLockWriteGuard<T> {
        self.0.write().unwrap()
    }

    /// Returns a shared reference to the inner store.
    ///
    /// # Panics
    ///
    /// Panics if the store is currently borrowed.
    pub fn borrow(&self) -> RwLockReadGuard<T> {
        self.0.read().unwrap()
    }
}

impl<T> Clone for Shared<T> {
    #[inline]
    fn clone(&self) -> Self {
        // we need this implementation rather than just deriving clone because
        // we don't need T to have Clone, we just clone the Arc
        Shared(self.0.clone())
    }
}

impl<T: Read> Read for Shared<T> {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.borrow().get(key)
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        self.borrow().get_next(key)
    }

    #[inline]
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        self.borrow().get_prev(key)
    }
}

impl<W: Write> Write for Shared<W> {
    #[inline]
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut store = self.borrow_mut();
        store.put(key, value)
    }

    #[inline]
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let mut store = self.borrow_mut();
        store.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::*;

    #[test]
    fn share() {
        let mut store = Shared::new(MapStore::new());
        let mut share0 = store.clone();
        let share1 = store.clone();

        share0.put(vec![123], vec![5]).unwrap();
        assert_eq!(store.get(&[123]).unwrap(), Some(vec![5]));
        assert_eq!(share0.get(&[123]).unwrap(), Some(vec![5]));
        assert_eq!(share1.get(&[123]).unwrap(), Some(vec![5]));

        store.put(vec![123], vec![6]).unwrap();
        assert_eq!(store.get(&[123]).unwrap(), Some(vec![6]));
        assert_eq!(share0.get(&[123]).unwrap(), Some(vec![6]));
        assert_eq!(share1.get(&[123]).unwrap(), Some(vec![6]));
    }

    #[test]
    fn iter() {
        let mut store = Shared::new(MapStore::new());

        store.put(vec![1], vec![10]).unwrap();
        store.put(vec![2], vec![20]).unwrap();
        store.put(vec![3], vec![30]).unwrap();

        let mut iter = store.into_iter(..);
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![10]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![20]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![3], vec![30]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn read_while_iter_exists() {
        let mut store = Shared::new(MapStore::new());
        let store2 = store.clone();

        store.put(vec![1], vec![10]).unwrap();
        store.put(vec![2], vec![20]).unwrap();
        store.put(vec![3], vec![30]).unwrap();

        let mut iter = store.into_iter(..);
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![10]));
        assert_eq!(store2.get(&[2]).unwrap(), Some(vec![20]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![20]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![3], vec![30]));
        assert!(iter.next().is_none());
    }
}
