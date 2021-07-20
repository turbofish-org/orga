use crate::error::Result;
use std::ops::{Bound, Deref, DerefMut, RangeBounds};

pub mod bufstore;
pub mod iter;
pub mod nullstore;
pub mod share;
#[allow(clippy::module_inception)]
pub mod store;

pub use bufstore::{BufStore, Map as BufStoreMap, MapStore};
pub use iter::Iter;
pub use nullstore::NullStore;
pub use share::Shared;
pub use store::{DefaultBackingStore, Store};

// TODO: Key type (for cheaper concat, enum over ref or owned slice, etc)

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

    /// Returns an iterator over the key/value entries in the given range.
    #[inline]
    fn range<B: RangeBounds<Vec<u8>>>(&self, bounds: B) -> Iter<Self> {
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
