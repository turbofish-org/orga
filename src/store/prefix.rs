use super::{Read, Write};
use crate::Result;

/// A `Store` which wraps another `Store` and appends a prefix byte to the key
/// for every read or write.
///
/// This can be useful to create a hierarchy of data within a single store -
/// effectively namespacing the keys to prevent key conflicts.
pub struct Prefixed<S> {
    store: S,
    prefix: u8
}

impl<S> Prefixed<S> {
    pub fn new(store: S, prefix: u8) -> Self {
        Prefixed { store, prefix }
    }
}

// TODO: make a Key type which doesn't need copying
#[inline]
fn prefix(prefix: u8, suffix: &[u8]) -> ([u8; 256], usize) {
    let mut prefixed = [0; 256];
    prefixed[0] = prefix;
    prefixed[1..suffix.len() + 1].copy_from_slice(suffix);
    (prefixed, suffix.len() + 1)
}

impl<R: Read> Read for Prefixed<R> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let (key, len) = prefix(self.prefix, key);
        self.store.get(&key[..len])
    }
}

impl<W: Write> Write for Prefixed<W> {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let (key, len) = prefix(self.prefix, key.as_slice());
        self.store.put(key[..len].to_vec(), value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let (key, len) = prefix(self.prefix, key);
        self.store.delete(&key[..len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::*;

    #[test]
    fn share() {
        let mut store = MapStore::new();

        let mut prefixed = (&mut store).prefix(123);
        prefixed.put(vec![5], vec![5]).unwrap();
        assert_eq!(prefixed.get(&[5]).unwrap(), Some(vec![5]));
        
        assert_eq!(store.get(&[123, 5]).unwrap(), Some(vec![5]));
    }
}
