use super::{Read, Write, Entry};
use crate::Result;

/// A `Store` which wraps another `Store` and appends a prefix byte to the key
/// for every read or write.
///
/// This can be useful to create a hierarchy of data within a single store -
/// effectively namespacing the keys to prevent key conflicts.
pub struct Prefixed<S> {
    store: S,
    prefix: u8,
}

impl<S> Prefixed<S> {
    /// Constructs a `Prefixed` by wrapping the given store and prepending keys
    /// with the given prefix for all operations.
    pub fn new(store: S, prefix: u8) -> Self {
        Prefixed { store, prefix }
    }
}

// TODO: make a Key type which doesn't need copying
/// Prepends a key with a prefix byte by copying into inline stack memory and
/// returning a fixed-size array.
#[inline]
fn prefix(prefix: u8, suffix: &[u8]) -> ([u8; 256], usize) {
    let mut prefixed = [0; 256];
    prefixed[0] = prefix;
    prefixed[1..suffix.len() + 1].copy_from_slice(suffix);
    (prefixed, suffix.len() + 1)
}

impl<S: Read> Read for Prefixed<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let (key, len) = prefix(self.prefix, key);
        self.store.get(&key[..len])
    }
}

impl<S: Write> Write for Prefixed<S> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let (key, len) = prefix(self.prefix, key.as_slice());
        self.store.put(key[..len].to_vec(), value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let (key, len) = prefix(self.prefix, key);
        self.store.delete(&key[..len])
    }
}

impl<S> super::Iter for Prefixed<S>
    where S: Read + super::Iter
{
    type Iter<'a> = Iter<'a, S::Iter<'a>>;

    fn iter_from(&self, start: &[u8]) -> Self::Iter<'_> {
        let (start, start_len) = prefix(self.prefix, start);
        let store_iter = self.store.iter_from(&start[..start_len]);
        Iter {
            inner: store_iter,
            prefix: self.prefix
        }
    }
}

pub struct Iter<'a, I>
    where I: Iterator<Item = Entry<'a>>
{
    inner: I,
    prefix: u8
}

impl<'a, I> Iterator for Iter<'a, I>
    where I: Iterator<Item = Entry<'a>>
{
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            None => None,
            Some((key, value)) => {
                if key[0] != self.prefix {
                    None
                } else {
                    Some((&key[1..], value))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{*, iter::Iter};

    #[test]
    fn share() {
        let mut store = MapStore::new();

        let mut prefixed = (&mut store).prefix(123);
        prefixed.put(vec![5], vec![5]).unwrap();
        assert_eq!(prefixed.get(&[5]).unwrap(), Some(vec![5]));

        assert_eq!(store.get(&[123, 5]).unwrap(), Some(vec![5]));
    }

    #[test]
    fn iter() {
        let mut store = MapStore::new();
        store.put(vec![99], vec![123]).unwrap();
        store.put(vec![100], vec![1]).unwrap();
        store.put(vec![100, 1], vec![2]).unwrap();
        store.put(vec![100, 2], vec![3]).unwrap();
        store.put(vec![101, 1, 2, 3], vec![123]).unwrap();

        let store = store.prefix(100);
        let mut iter = store.iter();

        assert_eq!(iter.next(), Some((&[][..], &[1][..])));
        assert_eq!(iter.next(), Some((&[1][..], &[2][..])));
        assert_eq!(iter.next(), Some((&[2][..], &[3][..])));
        assert_eq!(iter.next(), None);
    }
}
