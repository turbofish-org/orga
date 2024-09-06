use crate::error::Result;
use std::{
    any::Any,
    ops::{Bound, Deref, DerefMut, RangeBounds},
};
use thiserror::Error;

pub mod backingstore;
pub mod bufstore;
pub mod iter;
pub mod log;
pub mod null;
pub mod partialmap;
pub mod share;
#[allow(clippy::module_inception)]
pub mod store;

pub use backingstore::BackingStore;
pub use bufstore::{BufStore, Map as BufStoreMap, MapStore};
pub use iter::Iter;
pub use null::Empty;
pub use partialmap::PartialMapStore;
pub use share::Shared;
pub use store::{DefaultBackingStore, Store};

// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

/// Errors which may occur when reading from a store with partial data.
#[derive(Error, Debug)]
pub enum Error {
    /// The status of the entry for this key isn't known.
    #[error("Tried to read unknown store data with key {0:?}")]
    GetUnknown(Vec<u8>),
    /// The next key for the provided key isn't known.
    #[error("Tried to read unknown store data after key {0:?}")]
    GetNextUnknown(Vec<u8>),
    /// The previous key for the provided key isn't known.
    #[error("Tried to read unknown store data before key {0:?}")]
    GetPrevUnknown(Option<Vec<u8>>),
}

/// A key/value entry - the first element is the key and the second element is
/// the value.
pub type KV = (Vec<u8>, Vec<u8>);

/// Trait for read access to key/value stores.
pub trait Read {
    /// Gets a value by key.
    ///
    /// Implementations of `get` should return `None` when there is no value for
    /// the key rather than erroring.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Gets the key/value entry which comes directly after `key` in ascending
    /// key order, or `None` if there are no entries which follow.
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>>;

    /// Gets the entry at `key` if it exists, otherwise returns the next entry
    /// by ascending key order, or `None` if there are no entries which follow.
    fn get_next_inclusive(&self, key: &[u8]) -> Result<Option<KV>> {
        match self.get(key)? {
            Some(value) => Ok(Some((key.to_vec(), value))),
            None => self.get_next(key),
        }
    }

    /// Gets the key/value entry which comes directly before `key` in ascending
    /// key order, or `None` if there are no entries which precede it.
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>>;

    /// Gets the entry at `key` if it exists, otherwise returns the previous
    /// entry by ascending key order, or `None` if there are no entries which
    /// precede it.
    fn get_prev_inclusive(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        match key {
            Some(key) => match self.get(key)? {
                Some(value) => Ok(Some((key.to_vec(), value))),
                None => self.get_prev(Some(key)),
            },
            None => self.get_prev(None),
        }
    }

    /// Returns an iterator over the key/value entries in the given range.
    #[inline]
    fn into_iter<B: RangeBounds<Vec<u8>>>(self, bounds: B) -> Iter<Self>
    where
        Self: Sized,
    {
        Iter::new(
            self,
            (
                clone_bound(bounds.start_bound()),
                clone_bound(bounds.end_bound()),
            ),
        )
    }
}

/// Returns the same variant of bound but with an owned copy of its inner value.
fn clone_bound<T: Clone>(bound: Bound<&T>) -> Bound<T> {
    match bound {
        Bound::Unbounded => Bound::Unbounded,
        Bound::Included(key) => Bound::Included(key.clone()),
        Bound::Excluded(key) => Bound::Excluded(key.clone()),
    }
}

impl<R: Read, T: Deref<Target = R>> Read for T {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.deref().get(key)
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        self.deref().get_next(key)
    }

    #[inline]
    fn get_next_inclusive(&self, key: &[u8]) -> Result<Option<KV>> {
        self.deref().get_next_inclusive(key)
    }

    #[inline]
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        self.deref().get_prev(key)
    }

    #[inline]
    fn get_prev_inclusive(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        self.deref().get_prev_inclusive(key)
    }
}

/// Trait for write access to key/value stores.
pub trait Write: Read {
    /// Writes a key and value to the store.
    ///
    /// If a value already exists for the given key, implementations should
    /// overwrite the value.
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;

    /// Deletes the value with the given key.
    ///
    /// If no value exists for the given key, implementations should treat the
    /// operation as a no-op (but may still issue a call to `delete` to an
    /// underlying store).
    fn delete(&mut self, key: &[u8]) -> Result<()>;
}

impl<S: Write, T: DerefMut<Target = S>> Write for T {
    #[inline]
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.deref_mut().put(key, value)
    }

    #[inline]
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.deref_mut().delete(key)
    }
}

/// A trait with [Read] and [Write] as supertraits to enable dynamic dispatch
/// stores (see: [BackingStore::Other]).
pub trait ReadWrite: Read + Write + Any + Send + Sync + 'static {
    /// Converts the store into a boxed `dyn Any`.
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<T: Read + Write + Send + Sync + 'static> ReadWrite for T {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
