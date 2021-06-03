use crate::store::{Read, KV};
use crate::Result;
use std::ops::{Bound, RangeBounds};

// TODO: make reversible
pub struct Iter<'a, S: ?Sized> {
    parent: &'a S,
    bounds: (Bound<Vec<u8>>, Bound<Vec<u8>>),
    done: bool,
}

impl<'a, S: Read + ?Sized> Iter<'a, S> {
    pub fn new(parent: &'a S, bounds: (Bound<Vec<u8>>, Bound<Vec<u8>>)) -> Self {
        Iter {
            parent,
            bounds,
            done: false,
        }
    }

    fn get_next_inclusive(&self, key: &[u8]) -> Result<Option<KV>> {
        if let Some(value) = self.parent.get(key)? {
            return Ok(Some((key.to_vec(), value)));
        }

        self.parent.get_next(key)
    }
}

impl<'a, S: Read> Iterator for Iter<'a, S> {
    type Item = Result<KV>;

    fn next(&mut self) -> Option<Result<KV>> {
        if self.done {
            return None;
        }

        let maybe_entry = match self.bounds.0 {
            // if entry exists at empty key, emit that. if not, get next entry
            Bound::Unbounded => self.get_next_inclusive(&[]).transpose(),

            // if entry exists at given key, emit that. if not, get next entry
            Bound::Included(ref key) => self.get_next_inclusive(key).transpose(),

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
