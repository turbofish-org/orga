//! Iteration for stores.
use crate::store::{Read, KV};
use crate::Result;
use std::ops::{Bound, RangeBounds};

// TODO: make reversible (requires `get_prev` method on Read)

// TODO: should we continue attempting to read for iterations after reaching the
// end of store data if the end has not been reached? (e.g. kill `done`
// property). this will not usually happen since the data won't be mutated while
// the shared reference to parent exists, unless a store uses interior
// mutability

/// An iterator over key/value entries in a `Read` type.
///
/// `Iter` is typically created by calling `read.range(some_range)`.
///
/// Under the hood, the iterator calls `Read::get_next` and keeps track of its
/// current position.
pub struct Iter<S> {
    parent: S,
    bounds: (Bound<Vec<u8>>, Bound<Vec<u8>>),
    done: bool,
}

impl<S: Read> Iter<S> {
    /// Creates a new iterator over entries in `parent` in the given range
    /// bounds.
    pub fn new(parent: S, bounds: (Bound<Vec<u8>>, Bound<Vec<u8>>)) -> Self {
        Iter {
            parent,
            bounds,
            done: false,
        }
    }
}

impl<S: Read> Iterator for Iter<S> {
    type Item = Result<KV>;

    fn next(&mut self) -> Option<Result<KV>> {
        if self.done {
            return None;
        }

        let maybe_entry = match self.bounds.0 {
            // if entry exists at empty key, emit that. if not, get next entry
            Bound::Unbounded => self.parent.get_next_inclusive(&[]).transpose(),

            // if entry exists at given key, emit that. if not, get next entry
            Bound::Included(ref key) => self.parent.get_next_inclusive(key).transpose(),

            // get next entry
            Bound::Excluded(ref key) => self.parent.get_next(key).transpose(),
        };

        match maybe_entry {
            // bubble up errors
            Some(Err(err)) => Some(Err(err)),

            // got entry
            Some(Ok((key, value))) => {
                // entry is past end of range, mark iterator as done
                if !self.bounds.contains(&key) {
                    self.done = true;
                    return None;
                }

                // advance internal state to next key
                self.bounds.0 = Bound::Excluded(key.clone());
                Some(Ok((key, value)))
            }

            // reached end of iteration, mark iterator as done
            None => {
                self.done = true;
                None
            }
        }
    }
}

impl<S: Read> DoubleEndedIterator for Iter<S> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let maybe_entry = match self.bounds.1 {
            // get last entry
            Bound::Unbounded => self.parent.get_prev(None).transpose(),

            // if entry exists at given key, emit that. if not, get prev entry
            Bound::Included(ref key) => self.parent.get_prev_inclusive(Some(key)).transpose(),

            // get prev entry
            Bound::Excluded(ref key) => self.parent.get_prev(Some(key)).transpose(),
        };

        match maybe_entry {
            // bubble up errors
            Some(Err(err)) => Some(Err(err)),

            // got entry
            Some(Ok((key, value))) => {
                // entry is past start of range, mark iterator as done
                if !self.bounds.contains(&key) {
                    self.done = true;
                    return None;
                }

                // advance internal state to prev key
                self.bounds.1 = Bound::Excluded(key.clone());
                Some(Ok((key, value)))
            }

            // reached end of iteration, mark iterator as done
            None => {
                self.done = true;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MapStore, Write};
    use crate::Error;

    fn test_store() -> MapStore {
        let mut store = MapStore::new();
        store.put(vec![0], vec![0]).unwrap();
        store.put(vec![1], vec![1]).unwrap();
        store.put(vec![2], vec![2]).unwrap();
        store
    }

    #[test]
    fn iter_unbounded_unbounded() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Unbounded, Bound::Unbounded),
            done: false,
        };
        assert_eq!(iter.next().unwrap().unwrap(), (vec![0], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![2]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_included_existing() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Included(vec![0]), Bound::Unbounded),
            done: false,
        };
        assert_eq!(iter.next().unwrap().unwrap(), (vec![0], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![2]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_included_nonexistent() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Included(vec![0, 1]), Bound::Unbounded),
            done: false,
        };
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![2]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_excluded_existing() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Excluded(vec![0]), Bound::Unbounded),
            done: false,
        };
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![2]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_excluded_nonexistent() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Excluded(vec![0, 1]), Bound::Unbounded),
            done: false,
        };
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![2], vec![2]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_error() {
        struct ErrorStore;
        impl Read for ErrorStore {
            fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> {
                Err(Error::Store("get".into()))
            }

            fn get_next(&self, _key: &[u8]) -> Result<Option<KV>> {
                Err(Error::Store("get_next".into()))
            }

            fn get_prev(&self, _key: Option<&[u8]>) -> Result<Option<KV>> {
                Err(Error::Store("get_prev".into()))
            }
        }

        let mut iter = Iter {
            parent: ErrorStore,
            bounds: (Bound::Unbounded, Bound::Unbounded),
            done: false,
        };
        assert_eq!(
            iter.next().unwrap().unwrap_err().to_string(),
            "Store Error: get"
        );

        let mut iter = Iter {
            parent: ErrorStore,
            bounds: (Bound::Excluded(vec![]), Bound::Unbounded),
            done: false,
        };
        assert_eq!(
            iter.next().unwrap().unwrap_err().to_string(),
            "Store Error: get_next"
        );
    }

    #[test]
    fn iter_end_past_range() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Unbounded, Bound::Included(vec![1])),
            done: false,
        };
        assert_eq!(iter.next().unwrap().unwrap(), (vec![0], vec![0]));
        assert_eq!(iter.next().unwrap().unwrap(), (vec![1], vec![1]));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_done() {
        let store = test_store();
        let mut iter = Iter {
            parent: store,
            bounds: (Bound::Unbounded, Bound::Unbounded),
            done: true,
        };
        assert!(iter.next().is_none());
    }
}
