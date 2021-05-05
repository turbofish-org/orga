use super::{Read, Read2, Write, Write2, Entry};
use crate::Result;
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::Rc;
use std::mem::transmute;

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

impl<T: Read2> Read2 for Shared<T> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.0.borrow().get(key)
    }
}

impl<T: Read> Read for Shared<T> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.0.borrow().get(key)
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

impl<W: Write2> Write2 for Shared<W> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut store = self.0.borrow_mut();
        store.put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let mut store = self.0.borrow_mut();
        store.delete(key)
    }
}

impl<R> super::Iter for Shared<R>
    where R: Read + super::Iter
{
    type Iter<'a> = Iter<'a, R::Iter<'a>>;

    fn iter_from(&self, start: &[u8]) -> Self::Iter<'_> {
        // erase lifetime information
        let iter = unsafe {
            self.0.as_ptr().as_ref().unwrap().iter_from(start)
        };

        // borrow store so we still get runtime borrow checks
        let _ref = unsafe {
            transmute::<
                std::cell::Ref<'_, R>,
                std::cell::Ref<'static, ()>
            >(self.0.borrow())
        };

        Iter { _ref, iter }
    }
}

pub struct Iter<'a, I>
    where
        I: Iterator<Item = Entry<'a>>
{
    _ref: std::cell::Ref<'static, ()>,
    iter: I
}

impl<'a, I> Iterator for Iter<'a, I>
    where I: Iterator<Item = Entry<'a>>
{
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::store::{*, iter::Iter};

//     #[test]
//     fn share() {
//         let mut store = MapStore::new().into_shared();
//         let mut share0 = store.clone();
//         let share1 = store.clone();

//         share0.put(vec![123], vec![5]).unwrap();
//         assert_eq!(store.get(&[123]).unwrap(), Some(vec![5]));
//         assert_eq!(share0.get(&[123]).unwrap(), Some(vec![5]));
//         assert_eq!(share1.get(&[123]).unwrap(), Some(vec![5]));

//         store.put(vec![123], vec![6]).unwrap();
//         assert_eq!(store.get(&[123]).unwrap(), Some(vec![6]));
//         assert_eq!(share0.get(&[123]).unwrap(), Some(vec![6]));
//         assert_eq!(share1.get(&[123]).unwrap(), Some(vec![6]));
//     }

//     #[test]
//     fn iter() {
//         let mut store = MapStore::new().into_shared();

//         store.put(vec![1], vec![10]).unwrap();
//         store.put(vec![2], vec![20]).unwrap();
//         store.put(vec![3], vec![30]).unwrap();

//         let mut iter = store.iter();
//         assert_eq!(iter.next(), Some((&[1][..], &[10][..])));
//         assert_eq!(iter.next(), Some((&[2][..], &[20][..])));
//         assert_eq!(iter.next(), Some((&[3][..], &[30][..])));
//         assert_eq!(iter.next(), None);
//     }

//     #[test]
//     fn read_while_iter_exists() {
//         let mut store = MapStore::new().into_shared();
//         let store2 = store.clone();

//         store.put(vec![1], vec![10]).unwrap();
//         store.put(vec![2], vec![20]).unwrap();
//         store.put(vec![3], vec![30]).unwrap();

//         let mut iter = store.iter();
//         assert_eq!(iter.next(), Some((&[1][..], &[10][..])));
//         assert_eq!(store2.get(&[2]).unwrap(), Some(vec![20]));
//         assert_eq!(iter.next(), Some((&[2][..], &[20][..])));
//         assert_eq!(iter.next(), Some((&[3][..], &[30][..])));
//         assert_eq!(iter.next(), None);
//     }

//     #[test]
//     #[should_panic(expected = "already borrowed: BorrowMutError")]
//     fn write_while_iter_exists() {
//         let mut store = MapStore::new().into_shared();
//         let mut store2 = store.clone();

//         store.put(vec![1], vec![10]).unwrap();
//         store.put(vec![2], vec![20]).unwrap();
//         store.put(vec![3], vec![30]).unwrap();

//         let mut iter = store.iter();
//         assert_eq!(iter.next(), Some((&[1][..], &[10][..])));
//         store2.put(vec![2], vec![21]).unwrap();
//         assert_eq!(iter.next(), Some((&[2][..], &[20][..])));
//         assert_eq!(iter.next(), Some((&[3][..], &[30][..])));
//         assert_eq!(iter.next(), None);
//     }
// }
