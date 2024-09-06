//! Core `Store` type and traits
use serde::{Deserialize, Serialize};
use std::ops::{Bound, RangeBounds};

use super::{BackingStore, Iter, Read, Shared, Write, KV};
use crate::describe::Describe;
use crate::encoding::{Decode, Encode, LengthVec, Terminated};
use crate::migrate::Migrate;
use crate::query::FieldQuery;
use crate::state::State;
use crate::{orga, Error, Result};

// TODO: figure out how to let users set DefaultBackingStore, similar to setting
// the global allocator in the standard library

/// The default backing store used as the type parameter given to `Store`. This
/// is used to prevent generic parameters bubbling up to the application level
/// for state types when they often all use the same backing store.
pub type DefaultBackingStore = BackingStore;

/// Wraps a "backing store" (an implementation of `Read` and possibly `Write`),
/// and applies all operations to a certain part of the backing store's keyspace
/// by adding a prefix.
///
/// This type is how high-level state types interact with the store, since they
/// will often need to create substores (through the `store.sub(prefix)`
/// method).
#[derive(Default, Serialize, Deserialize, FieldQuery)]
pub struct Store<S = DefaultBackingStore> {
    #[serde(skip)]
    prefix: Vec<u8>,
    #[serde(skip)]
    store: Shared<S>,
}

impl Store {
    /// Creates a new store with a [MapStore] as its backing store.
    pub fn with_map_store() -> Self {
        use super::MapStore;
        Self::new(BackingStore::MapStore(Shared::new(MapStore::new())))
    }

    /// Creates a new store with a [PartialMapStore] as its backing store.
    pub fn with_partial_map_store() -> Self {
        use super::PartialMapStore;
        Self::new(BackingStore::PartialMapStore(Shared::new(
            PartialMapStore::new(),
        )))
    }

    /// Removes all entries in the given key range.
    pub fn remove_range<B: RangeBounds<Vec<u8>>>(&mut self, bounds: B) -> Result<()> {
        self.range(bounds).try_for_each(|entry| {
            let (k, _) = entry?;
            self.delete(&k)
        })
    }
}

impl Migrate for Store {}

impl<S> Describe for Store<S>
where
    Self: State + 'static,
{
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl<S> Encode for Store<S> {
    fn encode_into<W: std::io::Write>(&self, _dest: &mut W) -> ed::Result<()> {
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(0)
    }
}

impl<S: Default> Decode for Store<S> {
    fn decode<R: std::io::Read>(_input: R) -> ed::Result<Self> {
        Ok(Self::default())
    }
}

impl<S> Terminated for Store<S> {}

impl<S> Clone for Store<S> {
    fn clone(&self) -> Self {
        Store {
            prefix: self.prefix.clone(),
            store: self.store.clone(),
        }
    }
}

impl<S: Read> Store<S> {
    /// Creates a new `Store` with no prefix, with `backing` as its backing
    /// store.
    #[inline]
    pub fn new(backing: S) -> Self {
        Store {
            prefix: vec![],
            store: Shared::new(backing),
        }
    }

    /// Creates a substore of this store by concatenating `prefix` to this
    /// store's own prefix, and pointing to the same backing store.
    #[inline]
    #[must_use]
    pub fn sub(&self, prefix: &[u8]) -> Self {
        Store {
            prefix: concat(self.prefix.as_slice(), prefix),
            store: self.store.clone(),
        }
    }

    /// Returns the prefix of this store.
    pub fn prefix(&self) -> &[u8] {
        self.prefix.as_slice()
    }

    /// Overrides the store's prefix, potentially causing key collisions.
    pub fn with_prefix(&self, prefix: Vec<u8>) -> Self {
        let mut store = self.clone();
        store.prefix = prefix;

        store
    }

    /// Returns the backing store.
    pub fn backing_store(&self) -> Shared<S> {
        self.store.clone()
    }

    /// Consumes the store and returns the backing store.
    pub fn into_backing_store(self) -> Shared<S> {
        self.store
    }

    /// Returns an iterator over key/value entries in the store within the given
    /// key range.
    pub fn range<B: RangeBounds<Vec<u8>>>(&self, bounds: B) -> Iter<Self>
    where
        Self: Read,
    {
        Read::into_iter(self.clone(), bounds)
    }
}

#[orga]
impl Store {
    #[query]
    pub fn get_query(&self, key: LengthVec<u8, u8>) -> Result<Option<Vec<u8>>> {
        self.store.get(key.as_slice())
    }

    #[query]
    pub fn page(
        &self,
        start: LengthVec<u8, u8>,
        end: Option<LengthVec<u8, u8>>,
        limit: u32,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let limit = limit.min(100);

        let start = Bound::Included(start.to_vec());
        let end = match end {
            Some(end) => Bound::Included(end.to_vec()),
            None => Bound::Unbounded,
        };
        self.range((start, end)).take(limit as usize).collect()
    }
}

impl State for Store {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.prefix = store.prefix;
        self.store = store.store;
        Ok(())
    }

    fn load(store: Store, _bytes: &mut &[u8]) -> Result<Self> {
        let mut value = store.clone();
        value.attach(store)?;
        Ok(value)
    }

    fn flush<W: std::io::Write>(self, _out: &mut W) -> Result<()> {
        Ok(())
    }
}

impl<S: Read> Read for Store<S> {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let prefixed = concat(self.prefix.as_slice(), key);
        self.store.get(prefixed.as_slice())
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let prefixed = concat(self.prefix.as_slice(), key);
        let maybe_kv = self
            .store
            .get_next(prefixed.as_slice())?
            .filter(|(k, _)| k.starts_with(self.prefix.as_slice()))
            .map(|(k, v)| (k[self.prefix.len()..].into(), v));
        Ok(maybe_kv)
    }

    #[inline]
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        let maybe_kv = if let Some(key) = key {
            let prefixed = concat(self.prefix.as_slice(), key);
            self.store
                .get_prev(Some(prefixed.as_slice()))?
                .filter(|(k, _)| k.starts_with(self.prefix.as_slice()))
                .map(|(k, v)| (k[self.prefix.len()..].into(), v))
        } else {
            let incremented = increment_bytes(self.prefix.clone());
            let end_key = if !self.prefix.is_empty() {
                Some(incremented.as_slice())
            } else {
                None
            };
            self.store
                .get_prev(end_key)?
                .filter(|(k, _)| k.starts_with(self.prefix.as_slice()))
                .map(|(k, v)| (k[self.prefix.len()..].into(), v))
        };
        Ok(maybe_kv)
    }
}

impl<S: Write> Write for Store<S> {
    #[inline]
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        // merk has a hard limit of 256 bytes for keys, but it does not create
        // an error until comitting. we assert the key length here so that
        // writes will fail early rather than making the entire block fail. this
        // assertion can be removed if the merk key length limit is removed, or
        // if we instead check this statically using known encoding lengths via
        // ed.
        if key.len() + self.prefix.len() >= 256 {
            return Err(Error::Store("Store keys must be < 256 bytes".into()));
        }

        let prefixed = concat(self.prefix.as_slice(), key.as_slice());
        self.store.put(prefixed, value)
    }

    #[inline]
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let prefixed = concat(self.prefix.as_slice(), key);
        self.store.delete(prefixed.as_slice())
    }
}

#[inline]
fn concat(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(a.len() + b.len());
    value.extend_from_slice(a);
    value.extend_from_slice(b);
    value
}

#[inline]
fn increment_bytes(mut bytes: Vec<u8>) -> Vec<u8> {
    for byte in bytes.iter_mut().rev() {
        if *byte == 255 {
            *byte = 0;
        } else {
            *byte += 1;
            return bytes;
        }
    }

    bytes.push(0);
    if bytes.len() > 1 {
        bytes[0] += 1;
    }

    bytes
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::store::MapStore;

    #[test]
    fn sub() {
        let mut backing = MapStore::new();
        backing.put(vec![0, 0], vec![0]).unwrap();
        backing.put(vec![1, 0], vec![1]).unwrap();
        backing.put(vec![1, 1], vec![2]).unwrap();
        backing.put(vec![2, 0], vec![3]).unwrap();

        // substore
        let mut store = Store::new(&mut backing).sub(&[1]);
        assert_eq!(store.get(&[0]).unwrap().unwrap(), vec![1]);
        assert_eq!(store.get_next(&[0]).unwrap().unwrap(), (vec![1], vec![2]));
        assert!(store.get_next(&[1]).unwrap().is_none());
        store.put(vec![2], vec![2, 0]).unwrap();
        store.delete(&[1]).unwrap();
        assert!(backing.get(&[1, 1]).unwrap().is_none());
        assert_eq!(backing.get(&[1, 2]).unwrap().unwrap(), vec![2, 0]);

        backing.put(vec![1, 3, 0], vec![4]).unwrap();
        backing.put(vec![1, 3, 1], vec![5]).unwrap();

        // recursive substore
        let mut store = Store::new(&mut backing).sub(&[1]).sub(&[3]);
        assert_eq!(store.get(&[0]).unwrap().unwrap(), vec![4]);
        assert_eq!(store.get_next(&[0]).unwrap().unwrap(), (vec![1], vec![5]));
        assert!(store.get_next(&[1]).unwrap().is_none());
        store.put(vec![2], vec![5, 0]).unwrap();
        store.delete(&[1]).unwrap();
        assert!(backing.get(&[1, 3, 1]).unwrap().is_none());
        assert_eq!(backing.get(&[1, 3, 2]).unwrap().unwrap(), vec![5, 0]);
    }

    #[test]
    fn get_prev_empty_key() {
        let mut backing = MapStore::new();
        backing.put(vec![0, 0], vec![0]).unwrap();

        let store = Store::new(&mut backing);
        assert_eq!(
            store.get_prev(None).unwrap().unwrap(),
            (vec![0, 0], vec![0])
        );
    }

    #[test]
    fn remove_range() -> Result<()> {
        let mut store = Store::with_map_store();
        store.put(vec![1, 1, 1], vec![1])?;
        store.put(vec![1, 2, 3], vec![1])?;
        store.put(vec![1, 2, 0], vec![1])?;
        store.put(vec![1, 3, 2], vec![1])?;

        let mut sub = store.sub(&[1, 2]);
        sub.remove_range(..)?;

        assert!(store.get(&[1, 1, 1])?.is_some());
        assert!(store.get(&[1, 2, 3])?.is_none());
        assert!(store.get(&[1, 2, 0])?.is_none());
        assert!(store.get(&[1, 3, 2])?.is_some());

        store.remove_range(vec![1, 2]..)?;
        assert!(store.get(&[1, 1, 1])?.is_some());
        assert!(store.get(&[1, 3, 2])?.is_none());

        Ok(())
    }
}
