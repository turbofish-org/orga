use super::{Read, Write, KV};
use crate::Result;
use std::cell::RefCell;
use std::rc::Rc;

// TODO: we can probably use UnsafeCell instead of RefCell since operations are
// guaranteed not to interfere with each other (the borrows only last until the
// end of each call).

/// A shared reference to a store, allowing the store to be cloned and read from
/// or written to by multiple consumers.
///
/// `Shared` has the `Clone` trait - it is safe to clone references to the store
/// since `get`, `put`, and `delete` all operate atomically so there will never
/// be more than one reference borrowing the underlying store at a time.
pub struct Shared<T>(Rc<RefCell<T>>);

impl<T> Shared<T> {
    /// Constructs a `Shared` by wrapping the given store.
    #[inline]
    pub fn new(inner: T) -> Self {
        Shared(Rc::new(RefCell::new(inner)))
    }
}

impl<T> Clone for Shared<T> {
    #[inline]
    fn clone(&self) -> Self {
        // we need this implementation rather than just deriving clone because
        // we don't need T to have Clone, we just clone the Rc
        Shared(self.0.clone())
    }
}

impl<T: Read> Read for Shared<T> {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.0.borrow().get(key)
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        self.0.borrow().get_next(key)
    }
}

impl<W: Write> Write for Shared<W> {
    #[inline]
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut store = self.0.borrow_mut();
        store.put(key, value)
    }

    #[inline]
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let mut store = self.0.borrow_mut();
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

        let mut iter = store.range(..);
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

        let mut iter = store.range(..);
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![10]));
        assert_eq!(store2.get(&[2]).unwrap(), Some(vec![20]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![20]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![3], vec![30]));
        assert!(iter.next().is_none());
    }
}
