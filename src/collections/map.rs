//! A map collection backed by a store
use std::cmp::Ordering;
use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::collections::{btree_map, BTreeMap};
use std::iter::Peekable;
use std::marker::PhantomData;
use std::ops::{Bound, Deref, DerefMut, RangeBounds};

use crate::call::{Call, FieldCall};
use crate::describe::Describe;
use crate::migrate::Migrate;
use crate::orga;
use crate::query::{FieldQuery, Query};
use crate::state::State;
use crate::store::*;
use crate::{Error, Result};
use ed::*;
use serde::Serialize;

/// A key in a [Map], which retains its encoded representation.
#[derive(Clone, Debug)]
pub struct MapKey<K> {
    inner: K,
    inner_bytes: Vec<u8>,
}

impl<V> Deref for MapKey<V> {
    type Target = V;

    fn deref(&self) -> &V {
        &self.inner
    }
}

impl<K> Encode for MapKey<K> {
    fn encode(&self) -> ed::Result<Vec<u8>> {
        Ok(self.inner_bytes.clone())
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match dest.write_all(self.inner_bytes.as_slice()) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.inner_bytes.len())
    }
}

//implement deref for MapKey to deref into the inner type
//implement Encode for MapKey that just returns the inner_bytes
impl<K> MapKey<K> {
    /// Create a new [MapKey] from an encodable value.
    pub fn new<E: Encode>(key: E) -> Result<MapKey<E>> {
        let inner_bytes = Encode::encode(&key)?;
        Ok(MapKey {
            inner: key,
            inner_bytes,
        })
    }
}

impl<K> PartialEq for MapKey<K> {
    fn eq(&self, other: &Self) -> bool {
        self.inner_bytes == other.inner_bytes
    }
}

impl<K: Encode> PartialOrd for MapKey<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Encode> Ord for MapKey<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner_bytes.cmp(&other.inner_bytes)
    }
}

impl<K> Eq for MapKey<K> {}

/// A map collection which stores data in a backing key/value store.
///
/// Keys are encoded into bytes and values are stored at the resulting key, with
/// child key/value entries (if any) stored with the encoded key as their
/// prefix.
///
/// When values in the map are mutated, inserted, or deleted, they are retained
/// in an in-memory map until the call to `State::flush` which writes the
/// changes to the backing store.
#[derive(FieldQuery, FieldCall)]
pub struct Map<K, V> {
    pub(super) store: Store,
    children: BTreeMap<MapKey<K>, Option<V>>,
}

impl<K, V> std::fmt::Debug for Map<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Map").finish()
    }
}

impl<K, V> Terminated for Map<K, V> {}

impl<K, V> State for Map<K, V>
where
    K: Encode + Terminated + 'static,
    V: State,
{
    fn attach(&mut self, store: Store) -> Result<()> {
        for (key, value) in self.children.iter_mut() {
            value.attach(store.sub(key.inner_bytes.as_slice()))?;
        }
        self.store.attach(store)
    }

    fn flush<W: std::io::Write>(mut self, _out: &mut W) -> Result<()> {
        while let Some((key, maybe_value)) = self.children.pop_first() {
            Self::apply_change(&mut self.store, key.inner.encode()?, maybe_value)?;
        }

        Ok(())
    }

    fn load(store: Store, _bytes: &mut &[u8]) -> Result<Self> {
        let mut map = Self::default();
        map.attach(store)?;

        Ok(map)
    }
}

impl<K, V> Map<K, V> {
    /// Create a new, empty [Map].
    pub fn new() -> Self {
        Self::default()
    }
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Map {
            store: Store::default(),
            children: BTreeMap::default(),
        }
    }
}

#[orga]
impl<K, V> Map<K, V>
where
    K: Encode + Terminated + Send + Sync + 'static,
    V: State,
{
    #[query]
    pub fn contains_key(&self, key: K) -> Result<bool> {
        let map_key = MapKey::<K>::new(key)?;
        let child_contains = self.children.contains_key(&map_key);

        if child_contains {
            let entry = self.children.get(&map_key);
            Ok(matches!(entry, Some(Some(_))))
        } else {
            let store_contains = match self.get_from_store(&map_key.inner)? {
                Some(..) => true,
                None => false,
            };

            Ok(store_contains)
        }
    }

    /// Gets the value from the key/value store by reading and decoding from raw
    /// bytes, then constructing a `State` instance for the value by creating a
    /// substore which uses the key as a prefix.
    fn get_from_store(&self, key: &K) -> Result<Option<V>> {
        let key_bytes = key.encode()?;
        self.store
            .get(key_bytes.as_slice())?
            .map(|value_bytes| {
                let substore = self.store.sub(key_bytes.as_slice());
                let mut value_bytes = value_bytes.as_slice();
                let value = V::load(substore, &mut value_bytes)?;
                debug_assert!(
                    value_bytes.is_empty(),
                    "Value had leftover bytes after decode"
                );
                Ok(value)
            })
            .transpose()
    }

    /// Insert a value at the provided key, replacing any existing value that
    /// may have been stored at that key.
    pub fn insert(&mut self, key: K, mut value: V) -> Result<()> {
        let map_key = MapKey::<K>::new(key)?;

        let substore = self.store.sub(map_key.inner_bytes.as_slice());
        value.attach(substore)?;
        self.children.insert(map_key, Some(value));

        Ok(())
    }

    /// Gets a reference to the value in the map for the given key, or `None` if
    /// the key has no value.
    ///
    /// The returned value will reference the latest changes to the data even if
    /// the value was inserted, modified, or deleted since the last time the map
    /// was flushed.
    #[query]
    pub fn get(&self, key: K) -> Result<Option<Ref<V>>> {
        let map_key = MapKey::<K>::new(key)?;
        Ok(if self.children.contains_key(&map_key) {
            // value is already retained in memory (was modified)
            self.children
                .get(&map_key)
                .unwrap()
                .as_ref()
                .map(Ref::Borrowed)
        } else {
            // value is not in memory, try to get from store
            self.get_from_store(&map_key.inner)?.map(Ref::Owned)
        })
    }
}

impl<K: Serialize, V: Serialize> Serialize for Map<K, V>
where
    K: Encode + Decode + Terminated + Clone + 'static,
    V: State,
{
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeSeq};
        let mut seq = serializer.serialize_seq(None)?;
        for entry in self.iter().map_err(Error::custom)? {
            let (key, value) = entry.map_err(Error::custom)?;
            seq.serialize_element(&(&*key, &*value))?;
        }
        seq.end()
    }
}

impl<K, V> Migrate for Map<K, V>
where
    K: Encode + Decode + State + Terminated + Clone + Send + Sync + Migrate,
    V: State + Migrate,
{
    fn migrate(mut src: Store, dest: Store, _bytes: &mut &[u8]) -> Result<Self> {
        let mut map = Map::with_store(dest.clone())?;

        for entry in StoreNextIter::<Store, K>::new(&src.clone(), ..)? {
            let (k, v) = entry?;
            let key = K::migrate(Store::default(), Store::default(), &mut k.as_slice())?;
            let value = V::migrate(src.sub(&k), dest.sub(&k), &mut v.as_slice())?;
            map.insert(key, value)?;
            Self::apply_change(&mut src, k, None)?;
            // TODO: flush the changes to the dest as we go - we are caching
            // changes in memory for now while we phase out old
            // migration implementations that don't honor the contract
        }

        Ok(map)
    }
}

impl<K, V> Map<K, V>
where
    K: Encode + Terminated + Send + Sync + 'static,
    V: State + Default,
{
    /// Returns a [Ref] of the value at the given key, or of the default
    /// value if the key was not present.
    pub fn get_or_default(&self, key: K) -> Result<Ref<V>> {
        let key_bytes = key.encode()?;
        let maybe_value = self.get(key)?;

        let value = match maybe_value {
            Some(value) => value,
            None => {
                let mut value = V::default();
                let substore = self.store.sub(key_bytes.as_slice());
                value.attach(substore)?;
                Ref::Owned(value)
            }
        };

        Ok(value)
    }
}

impl<K, V> Describe for Map<K, V>
where
    K: Encode + Terminated + Clone + 'static + Describe,
    V: State + Describe,
{
    fn describe() -> crate::describe::Descriptor {
        use crate::describe::Builder;
        Builder::new::<Self>()
            .dynamic_child::<K, V>(|mut query_bytes| {
                query_bytes.extend_from_slice(&[129]);
                query_bytes
            })
            .build()
    }
}

#[orga]
impl<K, V> Map<K, V>
where
    K: Encode + Terminated + Clone + Send + Sync + 'static,
    V: State,
{
    /// Gets a mutable reference to the value in the map for the given key, or
    /// `None` if the key has no value.
    ///
    /// If the value is mutated, it will be retained in memory until the map is
    /// flushed.
    ///
    /// The returned value will reference the latest changes to the data even if
    /// the value was inserted, modified, or deleted since the last time the map
    /// was flushed.
    // TODO: #[call]
    pub fn get_mut(&mut self, key: K) -> Result<Option<ChildMut<K, V>>> {
        Ok(self.entry(key)?.into())
    }

    /// Returns a mutable reference to the key/value entry for the given key.
    pub fn entry(&mut self, key: K) -> Result<Entry<K, V>> {
        let map_key = MapKey::<K>::new(key)?;
        Ok(if self.children.contains_key(&map_key) {
            // value is already retained in memory (was modified)
            let entry = match self.children.entry(map_key) {
                btree_map::Entry::Occupied(entry) => entry,
                _ => unreachable!(),
            };
            let child = ChildMut::Modified(entry);
            Entry::Occupied { child }
        } else {
            // value is not in memory, try to get from store
            match self.get_from_store(&map_key.inner)? {
                Some(value) => {
                    let kvs = (map_key.inner, value, self);
                    let child = ChildMut::Unmodified(Some(kvs));
                    Entry::Occupied { child }
                }
                None => Entry::Vacant {
                    key: map_key.inner,
                    parent: self,
                },
            }
        })
    }

    /// Removes the value at the given key, if any.
    pub fn remove(&mut self, key: K) -> Result<Option<ReadOnly<V>>> {
        let map_key = MapKey::<K>::new(key)?;
        if self.children.contains_key(&map_key) {
            let result = self.children.remove(&map_key).unwrap();
            self.children.insert(map_key, None);
            match result {
                Some(val) => Ok(Some(ReadOnly::new(val))),
                None => Ok(None),
            }
        } else {
            Ok(self.get_from_store(&map_key.inner)?.map(|val| {
                self.children.insert(map_key, None);
                ReadOnly::new(val)
            }))
        }
    }

    fn remove_raw(&mut self, k: K) -> Result<Option<V>> {
        let map_key = MapKey::<K>::new(k)?;
        if self.children.contains_key(&map_key) {
            let result = self.children.remove(&map_key).unwrap();
            self.children.insert(map_key, None);
            Ok(result)
        } else {
            Ok(self.get_from_store(&map_key.inner)?.map(|val| {
                self.children.insert(map_key, None);
                val
            }))
        }
    }

    /// Swaps the values at the given keys.
    pub fn swap(&mut self, i: K, j: K) -> Result<()> {
        if !(self.contains_key(i.clone())? && self.contains_key(j.clone())?) {
            return Err(Error::App("Swap failed. Key not found.".into()));
        }

        let a = self.remove_raw(i.clone())?;
        let b = self.remove_raw(j.clone())?;

        self.children.insert(MapKey::<K>::new(i)?, b);
        self.children.insert(MapKey::<K>::new(j)?, a);
        Ok(())
    }
}

impl<'a, K, V> Map<K, V>
where
    K: Encode + Decode + Terminated + Clone + 'static,
    V: State,
{
    /// Create an iterator over all KV pairs in the map.
    pub fn iter(&'a self) -> Result<Iter<'a, K, V>> {
        self.range(..)
    }

    /// Create an iterator over all KV pairs in the map within the given key
    /// range.
    pub fn range<B: RangeBounds<K>>(&'a self, range: B) -> Result<Iter<'a, K, V>> {
        let map_start = range
            .start_bound()
            .map(|inner| MapKey::<K>::new(inner.clone()).unwrap());
        let map_end = range
            .end_bound()
            .map(|inner| MapKey::<K>::new(inner.clone()).unwrap());
        let map_iter = self.children.range((map_start, map_end)).peekable();

        let encoded_range = (
            encode_bound(range.start_bound())?,
            encode_bound(range.end_bound())?,
        );
        let store_iter = StoreNextIter::new(&self.store, encoded_range)?;

        Ok(Iter {
            parent: self,
            map_iter,
            store_iter,
        })
    }
}

fn encode_bound<K: Encode>(bound: Bound<&K>) -> Result<Bound<Vec<u8>>> {
    match bound {
        Bound::Included(inner) => Ok(Bound::Included(inner.encode()?)),
        Bound::Excluded(inner) => Ok(Bound::Excluded(inner.encode()?)),
        Bound::Unbounded => Ok(Bound::Unbounded),
    }
}

impl<K, V> Map<K, V>
where
    K: Encode + Terminated + 'static,
    V: State,
{
    /// Removes all values with the given prefix from the key/value store.
    /// Iterates until reaching the first key that does not have the given
    /// prefix, or the end of the store.
    ///
    /// This method is used to delete a child value and all of its child entries
    /// (if any).
    fn remove_from_store(store: &mut Store, prefix: &[u8]) -> Result<bool> {
        let mut to_delete = vec![];
        for entry in store.range(prefix.to_vec()..) {
            let (key, _) = entry?;
            if !key.starts_with(prefix) {
                break;
            }
            to_delete.push(key);
        }

        let exists = !to_delete.is_empty();
        for key in to_delete {
            store.delete(key.as_slice())?;
        }

        Ok(exists)
    }

    /// Writes a change to the key/value store for the given key. If
    /// `maybe_value` is `Some`, the value's `State::flush` implementation is
    /// called then its binary encoding is written to `key`. If `maybe_value` is
    /// `None`, the value is removed by deleting all entries which start with
    /// `key`.
    fn apply_change(store: &mut Store, key_bytes: Vec<u8>, maybe_value: Option<V>) -> Result<()> {
        match maybe_value {
            Some(value) => {
                // insert/update
                let mut value_bytes = vec![];
                value.flush(&mut value_bytes)?;
                store.put(key_bytes, value_bytes)?;
            }
            None => {
                // delete
                Self::remove_from_store(store, key_bytes.as_slice())?;
            }
        }

        Ok(())
    }
}

/// An iterator over the elements of a [Map].
pub struct Iter<'a, K, V>
where
    K: Decode + Encode + Terminated + 'static,
    V: State,
{
    parent: &'a Map<K, V>,
    map_iter: Peekable<btree_map::Range<'a, MapKey<K>, Option<V>>>,
    store_iter: StoreNextIter<'a, Store, K>,
}

impl<'a, K, V> Iter<'a, K, V>
where
    K: Encode + Decode + Terminated + 'static,
    V: State,
{
    fn iter_merge_next(&mut self, forward: bool) -> Result<Option<(Ref<'a, K>, Ref<'a, V>)>> {
        loop {
            let (map_entry, backing_entry) = if forward {
                (self.map_iter.peek().cloned(), self.store_iter.peek())
            } else {
                (self.peek_map_back(), self.store_iter.peek_back())
            };

            let mut map_next = || {
                if forward {
                    self.map_iter.next()
                } else {
                    self.map_iter.next_back()
                }
            };
            let mut store_next = || {
                if forward {
                    self.store_iter.next()
                } else {
                    self.store_iter.next_back()
                }
            };

            return Ok(match (map_entry, backing_entry) {
                // consumed both iterators, end here
                (None, None) => None,

                // consumed backing iterator, still have map values
                (Some(_), None) => {
                    match map_next().unwrap() {
                        // map value has not been deleted, emit value
                        (key, Some(value)) => {
                            Some((Ref::Borrowed(&key.inner), Ref::Borrowed(value)))
                        }

                        // map value is a delete, go to the next entry
                        (_, None) => continue,
                    }
                }

                // consumed map iterator, still have backing values
                (None, Some(res)) => {
                    res?;
                    let entry = store_next()
                        .transpose()?
                        .expect("Peek ensures this arm is unreachable");

                    let mut key_bytes = entry.0.as_slice();
                    let key = Decode::decode(&mut key_bytes)?;
                    debug_assert!(key_bytes.is_empty(), "Key had leftover bytes after decode");

                    let mut value_bytes = entry.1.as_slice();
                    let value =
                        V::load(self.parent.store.sub(entry.0.as_slice()), &mut value_bytes)?;
                    debug_assert!(
                        value_bytes.is_empty(),
                        "Value had leftover bytes after decode"
                    );

                    Some((Ref::Owned(key), Ref::Owned(value)))
                }

                // merge values from both iterators
                (Some(map_entry), Some(store_res)) => {
                    let map_key = map_entry.0;
                    let backing_key = match store_res {
                        Err(_) => {
                            return Err(Error::Store("Backing key does not exist".into()));
                        }
                        Ok((ref key, _)) => key,
                    };

                    let mut key_bytes = backing_key.as_slice();
                    let key = Decode::decode(&mut key_bytes)?;
                    debug_assert!(
                        key_bytes.is_empty(),
                        "Key had leftover bytes after decode: key={} leftover={}",
                        hex::encode(backing_key),
                        hex::encode(key_bytes),
                    );

                    // so compare backing_key with map_key.inner_bytes
                    let key_cmp = map_key.inner_bytes.cmp(backing_key);

                    // map_key is past backing_key, emit the backing entry
                    if (forward && key_cmp == Ordering::Greater)
                        || (!forward && key_cmp == Ordering::Less)
                    {
                        let entry = store_next().unwrap()?;

                        let mut value_bytes = entry.1.as_slice();
                        let value =
                            V::load(self.parent.store.sub(entry.0.as_slice()), &mut value_bytes)?;
                        debug_assert!(
                            value_bytes.is_empty(),
                            "Value had leftover bytes after decode"
                        );

                        return Ok(Some((Ref::Owned(key), Ref::Owned(value))));
                    }

                    // map_key == backing_key, map entry shadows backing entry
                    if key_cmp == Ordering::Equal {
                        store_next().transpose()?;
                    }

                    // map_key is before or at backing_key
                    match map_next().unwrap() {
                        (key, Some(value)) => {
                            Some((Ref::Borrowed(&key.inner), Ref::Borrowed(value)))
                        }

                        // map entry deleted in in-memory map, skip
                        (_, None) => continue,
                    }
                }
            });
        }
    }

    fn peek_map_back(&mut self) -> Option<(&'a MapKey<K>, &'a Option<V>)> {
        self.map_iter.next_back().map(|back_entry| {
            let maybe_front_entry = self.map_iter.next();

            self.map_iter = self
                .parent
                .children
                .range((
                    maybe_front_entry
                        .map_or(Bound::Included(back_entry.0), |(k, _)| Bound::Included(k)),
                    Bound::Included(back_entry.0),
                ))
                .peekable();

            back_entry
        })
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V>
where
    K: Decode + Encode + Terminated,
    V: State,
{
    type Item = Result<(Ref<'a, K>, Ref<'a, V>)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter_merge_next(true).transpose()
    }
}

impl<'a, K, V> DoubleEndedIterator for Iter<'a, K, V>
where
    K: Decode + Encode + Terminated,
    V: State,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter_merge_next(false).transpose()
    }
}

struct StoreNextIter<'a, S: Default + Read, K: Decode> {
    store: &'a S,
    next_key: Bound<Vec<u8>>,
    end_key: Bound<Vec<u8>>,
    _phantom: PhantomData<K>,
}

// TODO: dedupe this with the same code in Store
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

fn decrement_bytes(mut bytes: Vec<u8>) -> Option<Vec<u8>> {
    for byte in bytes.iter_mut().rev() {
        if *byte == 0 {
            *byte = 255;
        } else {
            *byte -= 1;
            return Some(bytes);
        }
    }

    if bytes.is_empty() {
        bytes.pop();
        Some(bytes)
    } else {
        None
    }
}

impl<'a, S: Default + Read, K: Decode> StoreNextIter<'a, S, K> {
    pub fn new<B: RangeBounds<Vec<u8>>>(store: &'a S, range: B) -> Result<Self> {
        Ok(StoreNextIter {
            store,
            next_key: encode_bound(range.start_bound())?,
            end_key: encode_bound(range.end_bound())?,
            _phantom: PhantomData,
        })
    }

    pub fn peek(&self) -> Option<Result<(Vec<u8>, Vec<u8>)>> {
        let get_res = match self.next_key.as_ref() {
            Bound::Included(key) => self.store.get_next_inclusive(key.as_slice()),
            Bound::Excluded(key) => self.store.get_next(key.as_slice()),
            Bound::Unbounded => self.store.get_next(&[]),
        };

        let (key, value) = match get_res {
            Err(e) => return Some(Err(e)),
            Ok(None) => return None,
            Ok(Some((key, value))) => (key, value),
        };

        match &self.end_key {
            Bound::Excluded(end) if key >= *end => return None,
            Bound::Included(end) if key > *end => return None,
            _ => {}
        };

        Some(Ok((key, value)))
    }

    pub fn peek_back(&self) -> Option<Result<(Vec<u8>, Vec<u8>)>> {
        let get_res = match self.end_key.as_ref() {
            Bound::Included(key) => self.store.get_prev_inclusive(Some(key.as_slice())),
            Bound::Excluded(key) => self.store.get_prev(Some(key.as_slice())),
            Bound::Unbounded => self.store.get_prev(None),
        };

        let (key, value) = match get_res {
            Err(e) => return Some(Err(e)),
            Ok(None) => return None,
            Ok(Some((key, value))) => (key, value),
        };

        match &self.next_key {
            Bound::Excluded(end) if key <= *end => return None,
            Bound::Included(end) if key < *end => return None,
            // FIXME: this is the one place where iterator ranges become
            // un-pure, due to the fact that we store the base value of
            // collections at the empty key, which overlaps with the descendant
            // keyspace.
            Bound::Unbounded if key.is_empty() => return None,
            _ => {}
        };

        // if we seeked to a sub-entry of the map key, we need to trim the key
        // to the top-level map key and get that entry
        let mut key_slice = key.as_slice();
        if let Err(err) = K::decode(&mut key_slice) {
            return Some(Err(err.into()));
        }
        if !key_slice.is_empty() {
            let trimmed_len = key.len() - key_slice.len();
            let trimmed_key = key[..trimmed_len].to_vec();
            match self.store.get(&trimmed_key) {
                Err(e) => return Some(Err(e)),
                Ok(None) => return Some(Err(Error::Store("Expected value".to_string()))),
                Ok(Some(value)) => return Some(Ok((trimmed_key, value))),
            }
        }

        Some(Ok((key, value)))
    }
}

impl<'a, S: Default + Read, K: Decode> Iterator for StoreNextIter<'a, S, K> {
    type Item = Result<(Vec<u8>, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.peek()?.map(|(key, value)| {
            self.next_key = Bound::Included(increment_bytes(key.clone()));
            (key, value)
        }))
    }
}

impl<'a, S: Default + Read, K: Decode> DoubleEndedIterator for StoreNextIter<'a, S, K> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(self.peek_back()?.map(|(key, value)| {
            self.end_key = decrement_bytes(key.clone())
                .map(Bound::Included)
                .unwrap_or(Bound::Excluded(vec![]));
            (key, value)
        }))
    }
}

/// A wrapper which only allows immutable access to its inner value.
pub struct ReadOnly<V> {
    inner: V,
}

impl<V> Deref for ReadOnly<V> {
    type Target = V;

    fn deref(&self) -> &V {
        &self.inner
    }
}

impl<V> ReadOnly<V> {
    /// Create a new [ReadOnly] wrapper around the given value.
    pub fn new(inner: V) -> Self {
        ReadOnly { inner }
    }

    /// Consume this [ReadOnly] wrapper, returning the inner value.
    pub fn into_inner(self) -> V {
        self.inner
    }
}

/// An immutable reference to an existing key or value in a collection.
#[derive(Debug)]
pub enum Ref<'a, V> {
    /// An existing value which was loaded from the store.
    Owned(V),

    /// A reference to an existing value which is being retained in memory
    /// because it was modified.
    Borrowed(&'a V),
}

impl<'a, V: Query> Query for Ref<'a, V> {
    type Query = V::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.deref().query(query)
    }
}

impl<'a, V> Query for Ref<'a, V> {
    default type Query = ();

    default fn query(&self, _query: Self::Query) -> Result<()> {
        Err(Error::Query("Bounds not met".into()))
    }
}

impl<'a, V> Deref for Ref<'a, V> {
    type Target = V;

    fn deref(&self) -> &V {
        match self {
            Ref::Owned(inner) => inner,
            Ref::Borrowed(value) => value,
        }
    }
}

impl<'a, V: Default> Default for Ref<'a, V> {
    fn default() -> Self {
        Ref::Owned(V::default())
    }
}

impl<'a, V: PartialEq> PartialEq for Ref<'a, V> {
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other)
    }
}

impl<'a, V: Eq> Eq for Ref<'a, V> {}

impl<'a, V: PartialOrd> PartialOrd for Ref<'a, V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<'a, V: Ord> Ord for Ref<'a, V> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deref().cmp(other)
    }
}

/// A mutable reference to an existing value in a collection.
///
/// If the value is mutated, it will be retained in memory until the parent
/// collection is flushed.
pub enum ChildMut<'a, K, V> {
    /// An existing value which was loaded from the store.
    Unmodified(Option<(K, V, &'a mut Map<K, V>)>),

    /// A mutable reference to an existing value which is being retained in
    /// memory because it was modified.
    Modified(btree_map::OccupiedEntry<'a, MapKey<K>, Option<V>>),
}

impl<'a, K, V> ChildMut<'a, K, V>
where
    K: Encode + Terminated + Clone + Send + Sync + 'static,
    V: State,
{
    /// Removes the value and all of its child key/value entries (if any) from
    /// the parent collection.
    pub fn remove(self) -> Result<()> {
        match self {
            ChildMut::Unmodified(mut inner) => {
                let (key, _, parent) = inner.take().unwrap();
                parent.remove(key)?;
            }
            ChildMut::Modified(mut entry) => {
                entry.insert(None);
            }
        };

        Ok(())
    }
}

impl<'a, K, V: Call> Call for ChildMut<'a, K, V>
where
    K: State + Clone + Encode,
{
    type Call = V::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        (**self).call(call)
    }
}

impl<'a, K: Encode, V> Deref for ChildMut<'a, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        match self {
            ChildMut::Unmodified(inner) => &inner.as_ref().unwrap().1,
            ChildMut::Modified(entry) => entry.get().as_ref().unwrap(),
        }
    }
}

impl<'a, K, V> DerefMut for ChildMut<'a, K, V>
where
    K: Clone + Encode,
{
    fn deref_mut(&mut self) -> &mut V {
        match self {
            ChildMut::Unmodified(inner) => {
                // insert into parent's children map and upgrade child to
                // Child::ModifiedMut
                let (key, value, parent) = inner.take().unwrap();

                let map_key = MapKey::<K>::new(key).unwrap();
                parent.children.insert(map_key.clone(), Some(value));
                let entry = parent.children.entry(map_key);

                let entry = match entry {
                    Occupied(occupied_entry) => occupied_entry,
                    Vacant(_) => unreachable!("Insert ensures Vacant variant is unreachable"),
                };
                *self = ChildMut::Modified(entry);
                self.deref_mut()
            }
            ChildMut::Modified(entry) => entry.get_mut().as_mut().unwrap(),
        }
    }
}

/// A mutable reference to a key/value entry in a collection, which may be
/// empty.
pub enum Entry<'a, K: Encode, V> {
    /// References an entry in the collection which does not have a value.
    Vacant {
        /// The key.
        key: K,
        /// The parent map.
        parent: &'a mut Map<K, V>,
    },

    /// References an entry in the collection which has a value.
    Occupied {
        /// The occupied key and value.
        child: ChildMut<'a, K, V>,
    },
}

impl<'a, K, V> Entry<'a, K, V>
where
    K: Encode + Terminated + Clone,
    V: State,
{
    /// If the `Entry` is empty, this method creates a new instance based on the
    /// given data. If not empty, this method returns a mutable reference to the
    /// existing data.
    ///
    /// Note that if a new instance is created, it will not be written to the
    /// store during the flush step unless the value gets modified. See
    /// `or_insert` for a variation which will always write the newly created
    /// value.
    pub fn or_create(self, mut value: V) -> Result<ChildMut<'a, K, V>> {
        Ok(match self {
            Entry::Vacant { key, parent } => {
                let key_bytes = key.encode()?;
                let substore = parent.store.sub(key_bytes.as_slice());
                value.attach(substore)?;
                ChildMut::Unmodified(Some((key, value, parent)))
            }
            Entry::Occupied { child } => child,
        })
    }

    /// If the `Entry` is empty, this method creates a new instance based on the
    /// given data. If not empty, this method returns a mutable reference to the
    /// existing data.
    ///
    /// Note that if a new instance is created, it will always be written to the
    /// store during the flush step even if the value never gets modified. See
    /// `or_create` for a variation which will only write the newly created
    /// value if it gets modified.
    pub fn or_insert(self, value: V) -> Result<ChildMut<'a, K, V>> {
        let mut child = self.or_create(value)?;
        child.deref_mut();
        Ok(child)
    }
}

impl<K: Encode + Decode + Terminated + 'static, V: State> Map<K, V> {
    /// Create a new, empty [Map] with the given backing [Store].
    pub fn with_store(store: Store) -> Result<Self> {
        let mut map = Map::new();
        State::attach(&mut map, store)?;
        Ok(map)
    }
}

impl<'a, K, V> Entry<'a, K, V>
where
    K: Encode + Terminated + Clone + Send + Sync + 'static,
    V: State,
{
    /// Removes the value for the `Entry` if it exists. Returns a boolean which
    /// is `true` if a value previously existed for the entry, `false`
    /// otherwise.
    pub fn remove(self) -> Result<bool> {
        Ok(match self {
            Entry::Occupied { child } => {
                child.remove()?;
                true
            }
            Entry::Vacant { .. } => false,
        })
    }
}

impl<'a, K, V> Entry<'a, K, V>
where
    K: Encode + Terminated + Clone,
    V: State + Default,
{
    /// If the `Entry` is empty, this method creates a new instance based on the
    /// default for the value's data encoding. If not empty, this method returns
    /// a mutable reference to the existing data.
    ///
    /// Note that if a new instance is created, it will not be written to the
    /// store during the flush step unless the value gets modified. See
    /// `or_insert_default` for a variation which will always write the newly
    /// created value.
    pub fn or_default(self) -> Result<ChildMut<'a, K, V>> {
        self.or_create(V::default())
    }

    /// If the `Entry` is empty, this method creates a new instance based on the
    /// default for the value's data encoding. If not empty, this method returns
    /// a mutable reference to the existing data.
    ///
    /// Note that if a new instance is created, it will always be written to the
    /// store during the flush step even if the value never gets modified. See
    /// `or_default` for a variation which will only write the newly created
    /// value if it gets modified.
    pub fn or_insert_default(self) -> Result<ChildMut<'a, K, V>> {
        #[allow(clippy::unwrap_or_default)]
        self.or_insert(V::default())
    }
}

impl<'a, K: Encode, V> From<Entry<'a, K, V>> for Option<ChildMut<'a, K, V>> {
    fn from(entry: Entry<'a, K, V>) -> Self {
        match entry {
            Entry::Vacant { .. } => None,
            Entry::Occupied { child } => Some(child),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::deque::Deque;
    use super::{Map, *};
    use crate::migrate::MigrateFrom;
    use crate::set_compat_mode;
    use crate::store::{MapStore, Store};

    fn enc(n: u32) -> Vec<u8> {
        Encode::encode(&n).unwrap()
    }

    fn setup() -> (Store, Map<u32, u32>) {
        let store = mapstore();
        let mut map: Map<u32, u32> = Default::default();
        map.attach(store.clone()).unwrap();
        (store, map)
    }

    // TODO: move this to store, backingstore, or testutils
    fn mapstore() -> Store {
        Store::new(Shared::new(MapStore::new()).into())
    }

    #[test]
    fn nonexistent() {
        let (store, mut map) = setup();
        assert!(map.get(3).unwrap().is_none());
        assert!(map.get_mut(3).unwrap().is_none());
        assert!(store.get(&enc(3)).unwrap().is_none());
    }

    #[test]
    fn store_only() {
        let (mut store, mut map) = setup();
        store.put(enc(1), enc(2)).unwrap();

        assert_eq!(*map.get(1).unwrap().unwrap(), 2);
        let mut v = map.get_mut(1).unwrap().unwrap();
        *v = 3;
        assert_eq!(store.get(&enc(1)).unwrap().unwrap(), enc(2));

        let mut buf = vec![];
        map.flush(&mut buf).unwrap();
        assert_eq!(store.get(&enc(1)).unwrap().unwrap(), enc(3));
    }

    #[test]
    fn mem_unmodified() {
        let (store, mut map) = setup();

        map.entry(4).unwrap().or_create(5).unwrap();
        assert!(map.get(4).unwrap().is_none());
        let map_key = MapKey::<u32>::new(4).unwrap();
        assert!(!map.children.contains_key(&map_key));
        assert!(store.get(&enc(4)).unwrap().is_none());
    }

    #[test]
    fn mem_modified() {
        let (store, mut map) = setup();

        let mut v = map.entry(6).unwrap().or_create(7).unwrap();
        *v = 8;
        assert_eq!(*map.get(6).unwrap().unwrap(), 8);
        let map_key = MapKey::<u32>::new(6).unwrap();
        assert!(map.children.contains_key(&map_key));
        assert!(store.get(&enc(6)).unwrap().is_none());

        let mut buf = vec![];
        map.flush(&mut buf).unwrap();
        assert_eq!(store.get(&enc(6)).unwrap().unwrap(), enc(8));
    }

    #[test]
    fn or_insert() {
        let (store, mut map) = setup();

        map.entry(9).unwrap().or_insert(10).unwrap();
        assert_eq!(*map.get(9).unwrap().unwrap(), 10);
        let map_key = MapKey::<u32>::new(9).unwrap();
        assert!(map.children.contains_key(&map_key));
        assert!(store.get(&enc(9)).unwrap().is_none());

        let mut buf = vec![];
        map.flush(&mut buf).unwrap();
        assert_eq!(store.get(&enc(9)).unwrap().unwrap(), enc(10));
    }

    #[test]
    fn or_insert_default() {
        let (store, mut map) = setup();

        map.entry(11).unwrap().or_insert_default().unwrap();
        assert_eq!(*map.get(11).unwrap().unwrap(), u32::default());
        let map_key = MapKey::<u32>::new(11).unwrap();
        assert!(map.children.contains_key(&map_key));
        assert!(store.get(&enc(11)).unwrap().is_none());

        let mut buf = vec![];
        map.flush(&mut buf).unwrap();
        assert_eq!(store.get(&enc(11)).unwrap().unwrap(), enc(u32::default()));
    }

    #[test]
    fn remove() {
        let (store, mut map) = setup();

        map.entry(12).unwrap().or_insert(13).unwrap();
        map.entry(14).unwrap().or_insert(15).unwrap();
        map.entry(16).unwrap().or_insert(17).unwrap();
        let map_key = MapKey::<u32>::new(12).unwrap();
        assert!(map.children.get(&map_key).unwrap().is_some());
        map.remove(12).unwrap();
        let map_key = MapKey::<u32>::new(12).unwrap();
        assert!(map.children.get(&map_key).unwrap().is_none());
        let mut buf = vec![];
        map.flush(&mut buf).unwrap();
        assert!(store.get(&enc(12)).unwrap().is_none());

        // Remove a value that was in the store before map's creation
        let mut map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();
        assert_eq!(*map.get(14).unwrap().unwrap(), 15);
        assert_eq!(*map.get(16).unwrap().unwrap(), 17);
        map.remove(14).unwrap();
        // Also remove a value by entry
        let entry = map.entry(16).unwrap();
        assert!(entry.remove().unwrap());
        let mut buf = vec![];
        map.flush(&mut buf).unwrap();
        assert!(store.get(&enc(14)).unwrap().is_none());
        assert!(store.get(&enc(16)).unwrap().is_none());
    }

    #[test]
    fn iter_merge_next_map_only() {
        let (_, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let map_iter = map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&map.store, ..).unwrap();

        let mut iter = Iter {
            parent: &map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 12);
                assert_eq!(*value, 24);
            }
            None => panic!("Expected Some"),
        }

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 13);
                assert_eq!(*value, 26);
            }
            None => panic!("Expected Some"),
        }

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 14);
                assert_eq!(*value, 28);
            }
            None => panic!("Expected Some"),
        }

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        assert!(iter_next.is_none());
    }

    #[test]
    fn iter_merge_next_store_only() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        assert_eq!(*iter_next.as_ref().unwrap().0, 12);
        assert_eq!(*iter_next.as_ref().unwrap().1, 24);

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        assert_eq!(*iter_next.as_ref().unwrap().0, 13);
        assert_eq!(*iter_next.as_ref().unwrap().1, 26);

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        assert_eq!(*iter_next.as_ref().unwrap().0, 14);
        assert_eq!(*iter_next.as_ref().unwrap().1, 28);

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        assert!(iter_next.is_none());
    }

    #[test]
    fn iter_merge_next_store_update() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        read_map.insert(12, 26).unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 12);
                assert_eq!(*value, 26);
            }
            None => panic!("Expected Some"),
        }
    }

    #[test]
    fn iter_merge_next_mem_remove() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        read_map.remove(12).unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        assert!(iter_next.is_none());
    }

    #[test]
    fn iter_merge_next_out_of_store_range() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 12);
                assert_eq!(*value, 24);
            }
            None => panic!("Expected Some"),
        }

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 14);
                assert_eq!(*value, 28);
            }
            None => {
                panic!("Expected Some")
            }
        }
    }

    #[test]
    fn iter_merge_next_store_key_in_map_range() {
        let (store, mut edit_map) = setup();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 12);
                assert_eq!(*value, 24);
            }
            None => panic!("Expected Some"),
        }

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 13);
                assert_eq!(*value, 26);
            }
            None => {
                panic!("Expected Some")
            }
        }
    }

    #[test]
    fn iter_merge_next_map_key_none() {
        let (store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();

        map.remove(12).unwrap();

        let map_iter = map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 13);
                assert_eq!(*value, 26);
            }
            None => panic!("Expected Some"),
        }
    }

    #[test]
    fn iter_merge_next_map_key_none_store_non_empty() {
        let (store, mut edit_map) = setup();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.remove(12).unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, true).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(*key, 13);
                assert_eq!(*value, 26);
            }
            None => panic!("Expected Some"),
        }
    }

    #[test]
    fn iter_merge_next_rev() {
        let (store, mut edit_map) = setup();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(15).unwrap().or_insert(26).unwrap();
        edit_map.entry(16).unwrap().or_insert(26).unwrap();
        edit_map.entry(17).unwrap().or_insert(26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store.clone()).unwrap();

        read_map.insert(12, 28).unwrap();
        read_map.insert(14, 28).unwrap();
        read_map.insert(16, 28).unwrap();
        read_map.entry(17).unwrap().remove().unwrap();

        let map_iter = read_map.children.range(..).peekable();
        let store_iter = StoreNextIter::new(&store, ..).unwrap();

        let mut iter = Iter {
            parent: &read_map,
            map_iter,
            store_iter,
        };

        let iter_next = Iter::iter_merge_next(&mut iter, false).unwrap().unwrap();
        assert_eq!(*iter_next.0, 16);
        assert_eq!(*iter_next.1, 28);

        let iter_next = Iter::iter_merge_next(&mut iter, false).unwrap().unwrap();
        assert_eq!(*iter_next.0, 15);
        assert_eq!(*iter_next.1, 26);

        let iter_next = Iter::iter_merge_next(&mut iter, false).unwrap().unwrap();
        assert_eq!(*iter_next.0, 14);
        assert_eq!(*iter_next.1, 28);

        let iter_next = Iter::iter_merge_next(&mut iter, false).unwrap().unwrap();
        assert_eq!(*iter_next.0, 13);
        assert_eq!(*iter_next.1, 26);

        let iter_next = Iter::iter_merge_next(&mut iter, false).unwrap().unwrap();
        assert_eq!(*iter_next.0, 12);
        assert_eq!(*iter_next.1, 28);

        let iter_next = Iter::iter_merge_next(&mut iter, false).unwrap();
        assert!(iter_next.is_none());
    }

    #[test]
    fn map_iter_map_only() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_store_only() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_mem_remove() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        read_map.remove(12).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_out_of_store_range() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_store_key_in_map_range() {
        let (store, mut edit_map) = setup();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_map_key_none() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();

        map.remove(12).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        map.iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_map_key_none_store_non_empty() {
        let (store, mut edit_map) = setup();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.remove(12).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_complex_rev() {
        let store = mapstore();
        let mut map: Map<u32, Map<u32, u32>> = Default::default();
        map.attach(store.clone()).unwrap();

        let mut submap = Map::new();
        submap.insert(1, 10).unwrap();
        submap.insert(2, 20).unwrap();
        submap.insert(3, 30).unwrap();
        map.insert(1, submap).unwrap();

        map.insert(2, Map::new()).unwrap();

        let mut submap = Map::new();
        submap.insert(4, 40).unwrap();
        submap.insert(5, 50).unwrap();
        map.insert(3, submap).unwrap();

        let mut bytes = vec![];
        map.flush(&mut bytes).unwrap();

        let map: Map<u32, Map<u32, u32>> = Map::with_store(store).unwrap();

        let mut iter = map.iter().unwrap().rev();

        let (key, submap) = iter.next().unwrap().unwrap();
        assert_eq!(*key, 3);
        let mut subiter = submap.iter().unwrap().rev().peekable();
        assert_eq!(*subiter.peek().unwrap().as_ref().unwrap().0, 5);
        assert_eq!(*subiter.next().unwrap().unwrap().1, 50);
        assert_eq!(*subiter.peek().unwrap().as_ref().unwrap().0, 4);
        assert_eq!(*subiter.next().unwrap().unwrap().1, 40);
        assert!(subiter.next().is_none());

        let (key, submap) = iter.next().unwrap().unwrap();
        assert_eq!(*key, 2);
        assert!(submap.iter().unwrap().next().is_none());

        let (key, submap) = iter.next().unwrap().unwrap();
        assert_eq!(*key, 1);
        let mut subiter = submap.iter().unwrap().rev().peekable();
        assert_eq!(*subiter.peek().unwrap().as_ref().unwrap().0, 3);
        assert_eq!(*subiter.next().unwrap().unwrap().1, 30);
        assert_eq!(*subiter.peek().unwrap().as_ref().unwrap().0, 2);
        assert_eq!(*subiter.next().unwrap().unwrap().1, 20);
        assert_eq!(*subiter.peek().unwrap().as_ref().unwrap().0, 1);
        assert_eq!(*subiter.next().unwrap().unwrap().1, 10);
        assert!(subiter.next().is_none());

        assert!(iter.next().is_none());
    }

    #[test]
    fn map_range_map_only_unbounded() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_start_bounded() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(13..)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_end_bounded_non_inclusive() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..13)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_end_bounded_inclusive() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..=13)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_bounded_non_inclusive() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(12..14)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_bounded_inclusive() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(12..=14)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_store_only_bounded_non_inclusive() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        read_map
            .range(12..14)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_store_only_bounded_inclusive() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let read_map: Map<u32, u32> = Map::with_store(store).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        read_map
            .range(12..=14)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_bounded() {
        let (store, mut edit_map) = setup();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();
        read_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        read_map
            .range(12..=14)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_empty() {
        let (_store, map) = setup();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..)
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_of_map() {
        let store = mapstore();
        let mut map: Map<u32, Map<u32, u32>> = Default::default();
        map.attach(store).unwrap();

        map.entry(42).unwrap().or_insert_default().unwrap();

        let mut sub_map = map.get_mut(42).unwrap().unwrap();
        sub_map.entry(13).unwrap().or_insert(26).unwrap();

        let inner_map = map.get(42).unwrap().unwrap();
        let actual = inner_map.get(13).unwrap().unwrap();

        assert_eq!(*actual, 26);
    }

    #[test]
    fn map_of_map_from_store() {
        let store = mapstore();
        let mut edit_map: Map<u32, Map<u32, u32>> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.entry(42).unwrap().or_insert_default().unwrap();

        let mut sub_map = edit_map.get_mut(42).unwrap().unwrap();
        sub_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let read_map: Map<u32, Map<u32, u32>> = Map::with_store(store).unwrap();
        let inner_map = read_map.get(42).unwrap().unwrap();
        let actual = inner_map.get(13).unwrap().unwrap();

        assert_eq!(*actual, 26);
    }

    #[test]
    fn map_of_deque() {
        let store = mapstore();
        let mut edit_map: Map<u32, Deque<u32>> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.entry(42).unwrap().or_insert_default().unwrap();

        let mut deque = edit_map.get_mut(42).unwrap().unwrap();
        deque.push_front(84).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, Deque<u32>> = Map::with_store(store).unwrap();
        let actual = read_map
            .get_mut(42)
            .unwrap()
            .unwrap()
            .pop_front()
            .unwrap()
            .unwrap();

        assert_eq!(*actual, 84);
    }

    #[test]
    fn map_of_map_iter() {
        let store = mapstore();
        let mut edit_map: Map<u32, Map<u32, u32>> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.entry(42).unwrap().or_insert_default().unwrap();
        let mut sub_map = edit_map.get_mut(42).unwrap().unwrap();
        sub_map.insert(13, 26).unwrap();
        sub_map.insert(14, 28).unwrap();

        edit_map.entry(43).unwrap().or_insert_default().unwrap();
        let mut sub_map = edit_map.get_mut(43).unwrap().unwrap();
        sub_map.insert(15, 30).unwrap();
        sub_map.insert(16, 32).unwrap();

        edit_map.entry(45).unwrap().or_insert_default().unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, Map<u32, u32>> = Default::default();
        read_map.attach(store).unwrap();

        let mut iter = read_map.iter().unwrap();

        let (key, sub_map) = iter.next().unwrap().unwrap();
        assert_eq!(*key, 42);
        assert_eq!(*sub_map.get(13).unwrap().unwrap(), 26);
        assert_eq!(*sub_map.get(14).unwrap().unwrap(), 28);

        let (key, sub_map) = iter.next().unwrap().unwrap();
        assert_eq!(*key, 43);
        assert_eq!(*sub_map.get(15).unwrap().unwrap(), 30);
        assert_eq!(*sub_map.get(16).unwrap().unwrap(), 32);

        let (key, _) = iter.next().unwrap().unwrap();
        assert_eq!(*key, 45);
    }

    #[test]
    fn map_insert() {
        let (_store, mut map) = setup();

        map.insert(12, 24).unwrap();
        map.insert(13, 26).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(2);

        map.iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_insert_complex_type() {
        let store = mapstore();
        let mut map: Map<u32, Deque<u32>> = Default::default();
        map.attach(store).unwrap();

        map.insert(12, Default::default()).unwrap();

        let mut deque = map.get_mut(12).unwrap().unwrap();
        deque.push_front(12).unwrap();

        assert_eq!(12, *deque.pop_front().unwrap().unwrap());
    }

    #[test]
    fn map_insert_with_flush() {
        let store = mapstore();
        let mut edit_map: Map<u32, u32> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.insert(12, 24).unwrap();
        edit_map.insert(13, 26).unwrap();

        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(2);

        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_insert_store_overwrite() {
        let store = mapstore();
        let mut edit_map: Map<u32, u32> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.insert(12, 24).unwrap();
        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();
        read_map.insert(12, 26).unwrap();

        assert_eq!(26, *read_map.get(12).unwrap().unwrap());
    }

    #[test]
    fn map_insert_store_overwrite_get_entry() {
        let store = mapstore();
        let mut edit_map: Map<u32, u32> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.insert(12, 24).unwrap();
        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();
        read_map.insert(12, 26).unwrap();

        let actual = read_map.entry(12).unwrap().or_insert(28).unwrap();
        assert_eq!(26, *actual);
    }

    // #[orga(version = 1)]
    // struct Foo {
    //     #[orga(version(V1))]
    //     bar: u32,

    //     baz: u32,
    // }
    // Recursive expansion of orga macro
    // ==================================

    #[derive(
        Default,
        ::orga::encoding::VersionedEncoding,
        ::orga::state::State,
        ::orga::serde::Serialize,
        ::orga::migrate::Migrate,
    )]
    #[state(version = 0u8)]
    #[encoding(version = 0u8)]
    #[migrate(version = 0u8)]
    struct FooV0 {
        baz: u32,
    }
    #[derive(
        Default,
        ::orga::encoding::VersionedEncoding,
        ::orga::state::State,
        ::orga::serde::Serialize,
        ::orga::migrate::Migrate,
        ::orga::call::FieldCall,
        ::orga::query::FieldQuery,
        ::orga::describe::Describe,
    )]
    #[state(version = 1u8, previous = "FooV0")]
    #[encoding(version = 1u8, previous = "FooV0")]
    #[migrate(version = 1u8, previous = "FooV0")]
    struct Foo {
        bar: u32,
        baz: u32,
    }
    type FooV1 = Foo;

    impl MigrateFrom<FooV0> for FooV1 {
        fn migrate_from(prev: FooV0) -> orga::Result<Self> {
            Ok(Self {
                bar: 0,
                baz: prev.baz,
            })
        }
    }

    #[test]
    fn migrate() {
        let mut store = mapstore();
        store.put(vec![0, 0, 0, 12], vec![0, 0, 0, 0, 123]).unwrap();

        let map = Map::<u32, Foo>::migrate(store.clone(), store, &mut &[][..]).unwrap();

        assert_eq!(map.get(12).unwrap().unwrap().bar, 0);
        assert_eq!(map.get(12).unwrap().unwrap().baz, 123);
    }

    #[test]
    fn migrate_compat_mode() {
        set_compat_mode(true);

        let mut store = mapstore();
        store.put(vec![0, 0, 0, 12], vec![0, 0, 0, 123]).unwrap();

        let map = Map::<u32, Foo>::migrate(store.clone(), store, &mut &[][..]).unwrap();

        set_compat_mode(false);

        assert_eq!(map.get(12).unwrap().unwrap().bar, 0);
        assert_eq!(map.get(12).unwrap().unwrap().baz, 123);
    }

    #[test]
    fn swap_in_memory() {
        let (_store, mut map) = setup();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        map.swap(13, 14).unwrap();
        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 28), (14, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn swap_in_store() {
        let store = mapstore();
        let mut edit_map: Map<u32, u32> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.insert(12, 24).unwrap();
        edit_map.insert(13, 26).unwrap();
        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();
        read_map.swap(12, 13).unwrap();

        let mut actual = vec![];
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 26), (13, 24)];
        assert_eq!(actual, expected);
    }

    #[test]
    fn swap_mem_store() {
        let store = mapstore();
        let mut edit_map: Map<u32, u32> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.insert(12, 24).unwrap();
        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();
        read_map.insert(13, 26).unwrap();
        read_map.swap(12, 13).unwrap();

        let mut actual = vec![];
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 26), (13, 24)];
        assert_eq!(actual, expected);
    }

    #[test]
    #[should_panic]
    fn swap_none() {
        let store = mapstore();
        let mut edit_map: Map<u32, u32> = Default::default();
        edit_map.attach(store.clone()).unwrap();

        edit_map.insert(12, 24).unwrap();
        let mut buf = vec![];
        edit_map.flush(&mut buf).unwrap();

        let mut read_map: Map<u32, u32> = Default::default();
        read_map.attach(store).unwrap();
        read_map.insert(13, 26).unwrap();
        read_map.swap(13, 26).unwrap();

        let mut actual = vec![];
        read_map
            .iter()
            .unwrap()
            .map(|result| result.unwrap())
            .for_each(|(k, v)| actual.push((*k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 26), (13, 24)];
        assert_eq!(actual, expected);
    }
}
