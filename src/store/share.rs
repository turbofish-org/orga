use super::{Read, Write};
use crate::Result;
use std::cell::RefCell;
use std::rc::Rc;

// TODO: we can probably use UnsafeCell instead of RefCell since operations are
// guaranteed not to interfere with each other.

/// A shared reference to a store, allowing the store to be cloned and read from
/// or written to by multiple consumers.
///
/// `Shared` has the `Clone` trait - it is safe to clone references to the store
/// since `get`, `put`, and `delete` all operate atomically so there will never
/// be more than one reference borrowing the underlying store at a time.
pub struct Shared<T>(Rc<RefCell<T>>);

impl<T> Shared<T> {
    /// Constructs a `Shared` by wrapping the given store.
    pub fn new(inner: T) -> Self {
        Shared(Rc::new(RefCell::new(inner)))
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Shared<T> {
        Self(self.0.clone())
    }
}

impl<R: Read> Read for Shared<R> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let store = self.0.borrow();
        store.get(key)
    }
}

impl<W: Write> Write for Shared<W> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut store = self.0.borrow_mut();
        store.put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let mut store = self.0.borrow_mut();
        store.delete(key)
    }
}

impl<'a, R> super::Iter<'a> for Shared<R>
    where R: Read + super::Iter<'a> + 'a
{
    type Iter = Iter<'a, R>;

    fn iter_from(&'a self, start: &[u8]) -> Self::Iter {
        let iter = unsafe {
            // erase lifetime information
            self.0.as_ptr().as_ref().unwrap().iter_from(start)
        };
        Iter {
            // we store an unused Ref to still get runtime borrow safety
            _ref: self.0.borrow(),
            iter
        }
    }
}

pub struct Iter<'a, R>
    where R: Read + super::Iter<'a> + 'a
{
    _ref: std::cell::Ref<'a, R>,
    iter: R::Iter
}

impl<'a, R> Iterator for Iter<'a, R>
    where R: Read + super::Iter<'a> + 'a
{
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{*, iter::Iter};

    #[test]
    fn share() {
        let mut store = MapStore::new().into_shared();
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
        let mut store = MapStore::new().into_shared();

        store.put(vec![1], vec![10]).unwrap();
        store.put(vec![2], vec![20]).unwrap();
        store.put(vec![3], vec![30]).unwrap();

        let mut iter = store.iter();
        assert_eq!(iter.next(), Some((&[1][..], &[10][..])));
        assert_eq!(iter.next(), Some((&[2][..], &[20][..])));
        assert_eq!(iter.next(), Some((&[3][..], &[30][..])));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn read_while_iter_exists() {
        let mut store = MapStore::new().into_shared();
        let store2 = store.clone();

        store.put(vec![1], vec![10]).unwrap();
        store.put(vec![2], vec![20]).unwrap();
        store.put(vec![3], vec![30]).unwrap();

        let mut iter = store.iter();
        assert_eq!(iter.next(), Some((&[1][..], &[10][..])));
        assert_eq!(store2.get(&[2]).unwrap(), Some(vec![20]));
        assert_eq!(iter.next(), Some((&[2][..], &[20][..])));
        assert_eq!(iter.next(), Some((&[3][..], &[30][..])));
        assert_eq!(iter.next(), None);
    }

    #[test]
    #[should_panic(expected = "already borrowed: BorrowMutError")]
    fn write_while_iter_exists() {
        let mut store = MapStore::new().into_shared();
        let mut store2 = store.clone();

        store.put(vec![1], vec![10]).unwrap();
        store.put(vec![2], vec![20]).unwrap();
        store.put(vec![3], vec![30]).unwrap();

        let mut iter = store.iter();
        assert_eq!(iter.next(), Some((&[1][..], &[10][..])));
        store2.put(vec![2], vec![21]).unwrap();
        assert_eq!(iter.next(), Some((&[2][..], &[20][..])));
        assert_eq!(iter.next(), Some((&[3][..], &[30][..])));
        assert_eq!(iter.next(), None);
    }
}
