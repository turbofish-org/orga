use serde::{Deserialize, Serialize};
use std::ops::RangeBounds;

use super::{Iter, Read, Shared, Write, KV};
use crate::encoding::{Decode, Encode, Terminated};
use crate::migrate::MigrateFrom;
use crate::state::State;
use crate::{Error, Result};

// TODO: figure out how to let users set DefaultBackingStore, similar to setting
// the global allocator in the standard library

/// The default backing store used as the type parameter given to `Store`. This
/// is used to prevent generic parameters bubbling up to the application level
/// for state types when they often all use the same backing store.
#[cfg(any(feature = "merk", feature = "merk-verify"))]
pub type DefaultBackingStore = crate::merk::BackingStore;
#[cfg(all(not(feature = "merk"), not(feature = "merk-verify")))]
pub type DefaultBackingStore = Shared<super::MapStore>;

/// Wraps a "backing store" (an implementation of `Read` and possibly `Write`),
/// and applies all operations to a certain part of the backing store's keyspace
/// by adding a prefix.
///
/// This type is how high-level state types interact with the store, since they
/// will often need to create substores (through the `store.sub(prefix)`
/// method).
#[derive(Default, Serialize, Deserialize)]
pub struct Store<S = DefaultBackingStore> {
    #[serde(skip)]
    prefix: Vec<u8>,
    #[serde(skip)]
    store: Shared<S>,
}

impl Store {
    #[cfg(feature = "merk")]
    pub fn with_map_store() -> Self {
        use super::MapStore;
        Self::new(crate::merk::BackingStore::MapStore(Shared::new(
            MapStore::new(),
        )))
    }

    #[cfg(not(feature = "merk"))]
    pub fn with_map_store() -> Self {
        use super::MapStore;
        Self::new(Shared::new(MapStore::new()))
    }
}

impl MigrateFrom for Store {
    fn migrate_from(other: Self) -> Result<Self> {
        Ok(other)
    }
}

// impl<S> Describe for Store<S>
// where
//     Self: State + 'static,
// {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>().build()
//     }
// }

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

impl<S> Store<S> {
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

    pub fn prefix(&self) -> &[u8] {
        self.prefix.as_slice()
    }

    /// # Safety
    /// Overrides the store's prefix, potentially causing key collisions.
    pub unsafe fn with_prefix(&self, prefix: Vec<u8>) -> Self {
        let mut store = self.clone();
        store.prefix = prefix;

        store
    }

    pub fn backing_store(&self) -> Shared<S> {
        self.store.clone()
    }

    pub fn into_backing_store(self) -> Shared<S> {
        self.store
    }

    pub fn range<B: RangeBounds<Vec<u8>>>(&self, bounds: B) -> Iter<Self>
    where
        Self: Read,
    {
        Read::into_iter(self.clone(), bounds)
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
}
