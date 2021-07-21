use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::{btree_map, BTreeMap};
use std::hash::Hash;
use std::iter::Peekable;
use std::ops::{Bound, Deref, DerefMut, RangeBounds};

use super::Next;
use crate::state::*;
use crate::store::*;
use crate::Result;
use ed::*;

/// A map collection which stores data in a backing key/value store.
///
/// Keys are encoded into bytes and values are stored at the resulting key, with
/// child key/value entries (if any) stored with the encoded key as their
/// prefix.
///
/// When values in the map are mutated, inserted, or deleted, they are retained
/// in an in-memory map until the call to `State::flush` which writes the
/// changes to the backing store.
pub struct Map<K, V, S = DefaultBackingStore> {
    store: Store<S>,
    children: BTreeMap<K, Option<V>>,
}

impl<K, V, S> From<Map<K, V, S>> for () {
    fn from(_map: Map<K, V, S>) {}
}

impl<K, V, S> State<S> for Map<K, V, S>
where
    K: Encode + Terminated + Eq + Hash + Ord,
    V: State<S>,
{
    type Encoding = ();

    fn create(store: Store<S>, _: ()) -> Result<Self> {
        Ok(Map {
            store,
            children: Default::default(),
        })
    }

    fn flush(mut self) -> Result<()>
    where
        S: Write,
    {
        for (key, maybe_value) in IntoIterator::into_iter(self.children) {
            Self::apply_change(&mut self.store, &key, maybe_value)?;
        }

        Ok(())
    }
}

impl<K, V, S> Map<K, V, S>
where
    K: Encode + Terminated + Eq + Hash + Ord + Copy,
    V: State<S>,
    S: Read,
{
    pub fn contains_key(&self, key: K) -> Result<bool> {
        let child_contains = self.children.contains_key(&key);

        if child_contains {
            Ok(child_contains)
        } else {
            let store_contains = match self.get_from_store(&key)? {
                Some(..) => true,
                None => false,
            };

            Ok(store_contains)
        }
    }

    /// Gets a reference to the value in the map for the given key, or `None` if
    /// the key has no value.
    ///
    /// The returned value will reference the latest changes to the data even if
    /// the value was inserted, modified, or deleted since the last time the map
    /// was flushed.
    pub fn get<B: Borrow<K>>(&self, key: B) -> Result<Option<Child<V>>> {
        let key = key.borrow();
        Ok(if self.children.contains_key(key) {
            // value is already retained in memory (was modified)
            self.children
                .get(key)
                .unwrap()
                .as_ref()
                .map(Child::Modified)
        } else {
            // value is not in memory, try to get from store
            self.get_from_store(key)?.map(Child::Unmodified)
        })
    }

    /// Gets a mutable reference to the value in the map for the given key, or
    /// `None` if the key has no value.
    ///
    /// If the value is mutated, it will be retained in memory until the map is
    /// flushed.
    ///
    /// The returned value will reference the latest changes to the data even if
    /// the value was inserted, modified, or deleted since the last time the map
    /// was flushed.
    pub fn get_mut(&mut self, key: K) -> Result<Option<ChildMut<K, V, S>>> {
        Ok(self.entry(key)?.into())
    }

    /// Returns a mutable reference to the key/value entry for the given key.
    pub fn entry(&mut self, key: K) -> Result<Entry<K, V, S>> {
        Ok(if self.children.contains_key(&key) {
            // value is already retained in memory (was modified)
            let entry = match self.children.entry(key) {
                btree_map::Entry::Occupied(entry) => entry,
                _ => unreachable!(),
            };
            let child = ChildMut::Modified(entry);
            Entry::Occupied { child }
        } else {
            // value is not in memory, try to get from store
            match self.get_from_store(&key)? {
                Some(value) => {
                    let kvs = (key, value, self);
                    let child = ChildMut::Unmodified(Some(kvs));
                    Entry::Occupied { child }
                }
                None => Entry::Vacant { key, parent: self },
            }
        })
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
                let decoded = V::Encoding::decode(value_bytes.as_slice())?;
                V::create(substore, decoded)
            })
            .transpose()
    }

    /// Removes the value at the given key, if any.
    pub fn remove(&mut self, key: K) -> Result<Option<ReadOnly<V>>> {
        if self.children.contains_key(&key) {
            let result = self.children.remove(&key).unwrap();
            self.children.insert(key, None);
            match result {
                Some(val) => Ok(Some(ReadOnly::new(val))),
                None => Ok(None),
            }
        } else {
            Ok(self.get_from_store(&key)?.map(|val| {
                self.children.insert(key, None);
                val.into()
            }))
        }
    }
}

impl<'a, 'b, K, V, S> Map<K, V, S>
where
    K: Encode + Decode + Terminated + Eq + Hash + Next<K> + Ord + Copy,
    V: State<S> + Decode + Copy,
    S: Read,
{
    fn iter_merge_next(
        map_iter: &mut Peekable<btree_map::Range<'a, K, Option<V>>>,
        store_iter: &mut Peekable<Iter<Store<S>>>,
    ) -> Result<Option<(K, Child<'a, V>)>> {
        loop {
            let has_map_entry = map_iter.peek().is_some();
            let has_backing_entry = store_iter.peek().is_some();

            return Ok(match (has_map_entry, has_backing_entry) {
                // consumed both iterators, end here
                (false, false) => None,

                // consumed backing iterator, still have map values
                (true, false) => {
                    match map_iter.next().unwrap() {
                        // map value has not been deleted, emit value
                        (key, Some(value)) => Some((*key, Child::Unmodified(*value))),

                        // map value is a delete, go to the next entry
                        (_, None) => continue,
                    }
                }

                // consumed map iterator, still have backing values
                (false, true) => {
                    match store_iter.next().transpose()? {
                        Some(entry) => {
                            let decoded_key: K = Decode::decode(entry.0.as_slice())?;
                            let decoded_value: V = Decode::decode(entry.1.as_slice())?;

                            Some((decoded_key, Child::Unmodified(decoded_value)))
                        }

                        // this should be unreachable considering the peek has returned that there
                        // are values in the backing
                        None => unreachable!("Peek ensures that this block is unreachable"),
                    }
                }

                // merge values from both iterators
                (true, true) => {
                    let map_key = map_iter.peek().unwrap().0;
                    let backing_key = match store_iter.peek().unwrap() {
                        Err(err) => failure::bail!("{}", err),
                        Ok((ref key, _)) => key,
                    };

                    let decoded_backing_key: K = Decode::decode(backing_key.as_slice())?;
                    let key_cmp = map_key.cmp(&decoded_backing_key);

                    // map_key > backing_key, emit the backing entry
                    // map_key == backing_key, map entry shadows backing entry
                    if key_cmp == Ordering::Greater || key_cmp == Ordering::Equal {
                        let entry = store_iter.next().unwrap()?;
                        let decoded_key: K = Decode::decode(entry.0.as_slice())?;
                        let encoded_key: V = Decode::decode(entry.1.as_slice())?;

                        return Ok(Some((decoded_key, Child::Unmodified(encoded_key))));
                    }

                    // map_key < backing_key
                    match map_iter.next().unwrap() {
                        (key, Some(value)) => Some((*key, Child::Modified(value))),

                        // map entry deleted in in-memory map, skip
                        (_, None) => continue,
                    }
                }
            });
        }
    }

    pub fn iter(&'a mut self) -> MapIterator<'a, K, V, S> {
        self.range(..)
    }

    pub fn range<B: RangeBounds<K> + Clone>(&'a mut self, range: B) -> MapIterator<'a, K, V, S> {
        let map_iter = self.children.range(range.clone()).peekable();
        let bounds = (
            encode_bound(range.start_bound()),
            encode_bound(range.end_bound()),
        );
        let store_iter = self.store.range(bounds).peekable();

        MapIterator {
            map_iter,
            store_iter,
        }
    }
}

fn encode_bound<K>(bound: Bound<&K>) -> Bound<Vec<u8>>
where
    K: Encode,
{
    match bound {
        Bound::Included(inner) => Bound::Included(Encode::encode(inner).unwrap()),
        Bound::Excluded(inner) => Bound::Excluded(Encode::encode(inner).unwrap()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

impl<K, V, S> Map<K, V, S>
where
    K: Encode + Terminated,
    V: State<S>,
    S: Write,
{
    /// Removes all values with the given prefix from the key/value store.
    /// Iterates until reaching the first key that does not have the given
    /// prefix, or the end of the store.
    ///
    /// This method is used to delete a child value and all of its child entries
    /// (if any).
    fn remove_from_store(store: &mut Store<S>, prefix: &[u8]) -> Result<bool> {
        let entries = store.range(prefix.to_vec()..);
        // TODO: make so we don't have to collect (should be able to delete
        // while iterating, either a .drain() iterator or an entry type with a
        // delete method)
        let to_delete: Vec<_> = entries
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(key, _)| key.to_vec())
            .collect();

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
    fn apply_change(store: &mut Store<S>, key: &K, maybe_value: Option<V>) -> Result<()> {
        let key_bytes = key.encode()?;

        match maybe_value {
            Some(value) => {
                // insert/update
                let value_bytes = value.flush()?.encode()?;
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

pub struct MapIterator<'a, K, V, S>
where
    K: Next<K> + Decode + Encode + Terminated + Hash + Eq,
    V: State<S>,
    S: Read,
{
    map_iter: Peekable<btree_map::Range<'a, K, Option<V>>>,
    store_iter: Peekable<Iter<'a, Store<S>>>,
}

impl<'a, K, V, S> Iterator for MapIterator<'a, K, V, S>
where
    K: Next<K> + Decode + Encode + Terminated + Hash + Eq + Ord + Copy,
    V: State<S> + Copy + Decode,
    S: Read,
{
    type Item = (K, Child<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        match Map::iter_merge_next(&mut self.map_iter, &mut self.store_iter) {
            Err(err) => panic!("{}", err),
            Ok(val) => val,
        }
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

impl<V> From<V> for ReadOnly<V> {
    fn from(value: V) -> Self {
        ReadOnly { inner: value }
    }
}

impl<V> ReadOnly<V> {
    fn new(inner: V) -> Self {
        ReadOnly { inner }
    }
}

/// An immutable reference to an existing value in a collection.
pub enum Child<'a, V> {
    /// An existing value which was loaded from the store.
    Unmodified(V),

    /// A reference to an existing value which is being retained in memory
    /// because it was modified.
    Modified(&'a V),
}

impl<'a, V> Deref for Child<'a, V> {
    type Target = V;

    fn deref(&self) -> &V {
        match self {
            Child::Unmodified(inner) => inner,
            Child::Modified(value) => value,
        }
    }
}

impl<'a, V: Default> Default for Child<'a, V> {
    fn default() -> Self {
        Child::Unmodified(V::default())
    }
}

/// A mutable reference to an existing value in a collection.
///
/// If the value is mutated, it will be retained in memory until the parent
/// collection is flushed.
pub enum ChildMut<'a, K, V, S> {
    /// An existing value which was loaded from the store.
    Unmodified(Option<(K, V, &'a mut Map<K, V, S>)>),

    /// A mutable reference to an existing value which is being retained in
    /// memory because it was modified.
    Modified(btree_map::OccupiedEntry<'a, K, Option<V>>),
}

impl<'a, K, V, S> ChildMut<'a, K, V, S>
where
    K: Hash + Eq + Encode + Terminated + Ord + Copy,
    V: State<S>,
    S: Read,
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

impl<'a, K: Ord, V, S> Deref for ChildMut<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        match self {
            ChildMut::Unmodified(inner) => &inner.as_ref().unwrap().1,
            ChildMut::Modified(entry) => entry.get().as_ref().unwrap(),
        }
    }
}

impl<'a, K, V, S> DerefMut for ChildMut<'a, K, V, S>
where
    K: Eq + Hash + Ord + Copy,
{
    fn deref_mut(&mut self) -> &mut V {
        match self {
            ChildMut::Unmodified(inner) => {
                // insert into parent's children map and upgrade child to
                // Child::ModifiedMut
                let (key, value, parent) = inner.take().unwrap();
                let _insertion = parent.children.insert(key, Some(value));
                let entry = parent.children.entry(key);
                match entry {
                    btree_map::Entry::Occupied(entry) => {
                        *self = ChildMut::Modified(entry);
                        self.deref_mut()
                    }
                    btree_map::Entry::Vacant(_entry) => {
                        panic!("Map insertion ensures this block is unreachable")
                    }
                }
            }
            ChildMut::Modified(entry) => entry.get_mut().as_mut().unwrap(),
        }
    }
}

/// A mutable reference to a key/value entry in a collection, which may be
/// empty.
pub enum Entry<'a, K: Copy, V, S> {
    /// References an entry in the collection which does not have a value.
    Vacant {
        key: K,
        parent: &'a mut Map<K, V, S>,
    },

    /// References an entry in the collection which has a value.
    Occupied { child: ChildMut<'a, K, V, S> },
}

impl<'a, K, V, S> Entry<'a, K, V, S>
where
    K: Encode + Terminated + Eq + Hash + Ord + Copy,
    V: State<S>,
    S: Read,
{
    /// If the `Entry` is empty, this method creates a new instance based on the
    /// given data. If not empty, this method returns a mutable reference to the
    /// existing data.
    ///
    /// Note that if a new instance is created, it will not be written to the
    /// store during the flush step unless the value gets modified. See
    /// `or_insert` for a variation which will always write the newly created
    /// value.
    pub fn or_create(self, data: V::Encoding) -> Result<ChildMut<'a, K, V, S>> {
        Ok(match self {
            Entry::Vacant { key, parent } => {
                let key_bytes = key.encode()?;
                let substore = parent.store.sub(key_bytes.as_slice());
                let value = V::create(substore, data)?;
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
    pub fn or_insert(self, data: V::Encoding) -> Result<ChildMut<'a, K, V, S>> {
        let mut child = self.or_create(data)?;
        child.deref_mut();
        Ok(child)
    }
}

impl<'a, K, V, S> Entry<'a, K, V, S>
where
    K: Encode + Terminated + Eq + Hash + Ord + Copy,
    V: State<S>,
    S: Write,
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

impl<'a, K, V, S, D> Entry<'a, K, V, S>
where
    K: Encode + Terminated + Eq + Hash + Ord + Copy,
    V: State<S, Encoding = D>,
    S: Read,
    D: Default,
{
    /// If the `Entry` is empty, this method creates a new instance based on the
    /// default for the value's data encoding. If not empty, this method returns
    /// a mutable reference to the existing data.
    ///
    /// Note that if a new instance is created, it will not be written to the
    /// store during the flush step unless the value gets modified. See
    /// `or_insert_default` for a variation which will always write the newly
    /// created value.
    pub fn or_default(self) -> Result<ChildMut<'a, K, V, S>> {
        self.or_create(D::default())
    }

    /// If the `Entry` is empty, this method creates a new instance based on the
    /// default for the value's data encoding. If not empty, this method returns
    /// a mutable reference to the existing data.
    ///
    /// Note that if a new instance is created, it will always be written to the
    /// store during the flush step even if the value never gets modified. See
    /// `or_default` for a variation which will only write the newly created
    /// value if it gets modified.
    pub fn or_insert_default(self) -> Result<ChildMut<'a, K, V, S>> {
        self.or_insert(D::default())
    }
}

impl<'a, K: Copy, V, S> From<Entry<'a, K, V, S>> for Option<ChildMut<'a, K, V, S>> {
    fn from(entry: Entry<'a, K, V, S>) -> Self {
        match entry {
            Entry::Vacant { .. } => None,
            Entry::Occupied { child } => Some(child),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::deque::*;
    use super::*;
    use crate::store::{MapStore, Store};

    fn enc(n: u32) -> Vec<u8> {
        n.encode().unwrap()
    }

    #[test]
    fn nonexistent() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();
        assert!(map.get(3).unwrap().is_none());
        assert!(map.get_mut(3).unwrap().is_none());
        assert!(store.get(&enc(3)).unwrap().is_none());
    }

    #[test]
    fn store_only() {
        let mut store = Store::new(MapStore::new());
        store.put(enc(1), enc(2)).unwrap();
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        assert_eq!(*map.get(1).unwrap().unwrap(), 2);
        let mut v = map.get_mut(1).unwrap().unwrap();
        *v = 3;
        assert_eq!(store.get(&enc(1)).unwrap().unwrap(), enc(2));

        map.flush().unwrap();
        assert_eq!(store.get(&enc(1)).unwrap().unwrap(), enc(3));
    }

    #[test]
    fn mem_unmodified() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(4).unwrap().or_create(5).unwrap();
        assert!(map.get(4).unwrap().is_none());
        assert_eq!(map.children.contains_key(&4), false);
        assert!(store.get(&enc(4)).unwrap().is_none());
    }

    #[test]
    fn mem_modified() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        let mut v = map.entry(6).unwrap().or_create(7).unwrap();
        *v = 8;
        assert_eq!(*map.get(6).unwrap().unwrap(), 8);
        assert!(map.children.contains_key(&6));
        assert!(store.get(&enc(6)).unwrap().is_none());

        map.flush().unwrap();
        assert_eq!(store.get(&enc(6)).unwrap().unwrap(), enc(8));
    }

    #[test]
    fn or_insert() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(9).unwrap().or_insert(10).unwrap();
        assert_eq!(*map.get(9).unwrap().unwrap(), 10);
        assert!(map.children.contains_key(&9));
        assert!(store.get(&enc(9)).unwrap().is_none());

        map.flush().unwrap();
        assert_eq!(store.get(&enc(9)).unwrap().unwrap(), enc(10));
    }

    #[test]
    fn or_insert_default() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(11).unwrap().or_insert_default().unwrap();
        assert_eq!(*map.get(11).unwrap().unwrap(), u32::default());
        assert!(map.children.contains_key(&11));
        assert!(store.get(&enc(11)).unwrap().is_none());

        map.flush().unwrap();
        assert_eq!(store.get(&enc(11)).unwrap().unwrap(), enc(u32::default()));
    }

    #[test]
    fn remove() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(13).unwrap();
        map.entry(14).unwrap().or_insert(15).unwrap();
        map.entry(16).unwrap().or_insert(17).unwrap();
        assert!(map.children.get(&12).unwrap().is_some());
        map.remove(12).unwrap();
        assert!(map.children.get(&12).unwrap().is_none());
        map.flush().unwrap();
        assert!(store.get(&enc(12)).unwrap().is_none());

        // Remove a value that was in the store before map's creation
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();
        assert_eq!(*map.get(14).unwrap().unwrap(), 15);
        assert_eq!(*map.get(16).unwrap().unwrap(), 17);
        map.remove(14).unwrap();
        // Also remove a value by entry
        let entry = map.entry(16).unwrap();
        assert!(entry.remove().unwrap());
        map.flush().unwrap();
        assert!(store.get(&enc(14)).unwrap().is_none());
        assert!(store.get(&enc(16)).unwrap().is_none());
    }

    #[test]
    fn iter_merge_next_map_only() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut map_iter = map.children.range(..).peekable();
        let mut range_iter = map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 12);
                assert_eq!(*value, 24);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 13);
                assert_eq!(*value, 26);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 14);
                assert_eq!(*value, 28);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some(_) => assert!(false),
            None => (),
        }
    }

    #[test]
    fn iter_merge_next_store_only() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        edit_map.flush().unwrap();

        let read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        let mut map_iter = read_map.children.range(..).peekable();
        let mut range_iter = read_map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 12);
                assert_eq!(*value, 24);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 13);
                assert_eq!(*value, 26);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 14);
                assert_eq!(*value, 28);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some(_) => assert!(false),
            None => (),
        }
    }

    #[test]
    fn iter_merge_next_mem_remove() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.remove(12).unwrap();

        let mut map_iter = read_map.children.range(..).peekable();
        let mut range_iter = read_map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 12);
                assert_eq!(*value, 24);
            }
            None => assert!(false),
        }
    }

    #[test]
    fn iter_merge_next_out_of_store_range() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut map_iter = read_map.children.range(..).peekable();
        let mut range_iter = read_map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 12);
                assert_eq!(*value, 24);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 14);
                assert_eq!(*value, 28);
            }
            None => {
                assert!(false)
            }
        }
    }

    #[test]
    fn iter_merge_next_store_key_in_map_range() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut map_iter = read_map.children.range(..).peekable();
        let mut range_iter = read_map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 12);
                assert_eq!(*value, 24);
            }
            None => assert!(false),
        }

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 13);
                assert_eq!(*value, 26);
            }
            None => {
                assert!(false)
            }
        }
    }

    #[test]
    fn iter_merge_next_map_key_none() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();

        map.remove(12).unwrap();

        let mut map_iter = map.children.range(..).peekable();
        let mut range_iter = map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 13);
                assert_eq!(*value, 26);
            }
            None => assert!(false),
        }
    }

    #[test]
    fn iter_merge_next_map_key_none_store_non_empty() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.remove(12).unwrap();

        let mut map_iter = read_map.children.range(..).peekable();
        let mut range_iter = read_map.store.range(..).peekable();

        let iter_next = Map::iter_merge_next(&mut map_iter, &mut range_iter).unwrap();
        match iter_next {
            Some((key, value)) => {
                assert_eq!(key, 13);
                assert_eq!(*value, 26);
            }
            None => assert!(false),
        }
    }

    #[test]
    fn map_iter_map_only() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.iter().for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_store_only() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);
        read_map.iter().for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_mem_remove() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.remove(12).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map.iter().for_each(|(x, y)| actual.push((x, *y)));

        let expected: Vec<(u32, u32)> = vec![(12, 24)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_out_of_store_range() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map.iter().for_each(|(x, y)| actual.push((x, *y)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_store_key_in_map_range() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map.iter().for_each(|(x, y)| actual.push((x, *y)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_map_key_none() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();

        map.remove(12).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        map.iter().for_each(|(x, y)| actual.push((x, *y)));

        let expected: Vec<(u32, u32)> = vec![(13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_iter_map_key_none_store_non_empty() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(13).unwrap().or_insert(26).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        read_map.entry(12).unwrap().or_insert(24).unwrap();
        read_map.remove(12).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(1);
        read_map.iter().for_each(|(x, y)| actual.push((x, *y)));

        let expected: Vec<(u32, u32)> = vec![(13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_unbounded() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_start_bounded() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(13..).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_end_bounded_non_inclusive() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..13).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_end_bounded_inclusive() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..=13).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_bounded_non_inclusive() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(12..14).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_map_only_bounded_inclusive() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        map.entry(12).unwrap().or_insert(24).unwrap();
        map.entry(13).unwrap().or_insert(26).unwrap();
        map.entry(14).unwrap().or_insert(28).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(12..=14).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_store_only_bounded_non_inclusive() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        read_map
            .range(12..14)
            .for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_store_only_bounded_inclusive() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(13).unwrap().or_insert(26).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        read_map
            .range(12..=14)
            .for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_bounded() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(12).unwrap().or_insert(24).unwrap();
        edit_map.entry(14).unwrap().or_insert(28).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();
        read_map.entry(13).unwrap().or_insert(26).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        read_map
            .range(12..=14)
            .for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![(12, 24), (13, 26), (14, 28)];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_range_empty() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, u32> = Map::create(store.clone(), ()).unwrap();

        let mut actual: Vec<(u32, u32)> = Vec::with_capacity(3);

        map.range(..).for_each(|(k, v)| actual.push((k, *v)));

        let expected: Vec<(u32, u32)> = vec![];

        assert_eq!(actual, expected);
    }

    #[test]
    fn map_of_map() {
        let store = Store::new(MapStore::new());
        let mut map: Map<u32, Map<u32, u32>> = Map::create(store.clone(), ()).unwrap();

        map.entry(42).unwrap().or_insert(()).unwrap();

        let mut sub_map = map.get_mut(42).unwrap().unwrap();
        sub_map.entry(13).unwrap().or_insert(26).unwrap();

        let inner_map = map.get(42).unwrap().unwrap();
        let actual = inner_map.get(13).unwrap().unwrap();

        assert_eq!(*actual, 26);
    }

    #[test]
    fn map_of_map_from_store() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, Map<u32, u32>> = Map::create(store.clone(), ()).unwrap();

        edit_map.entry(42).unwrap().or_insert(()).unwrap();

        let mut sub_map = edit_map.get_mut(42).unwrap().unwrap();
        sub_map.entry(13).unwrap().or_insert(26).unwrap();

        edit_map.flush().unwrap();

        let read_map: Map<u32, Map<u32, u32>> = Map::create(store.clone(), ()).unwrap();
        let inner_map = read_map.get(42).unwrap().unwrap();
        let actual = inner_map.get(13).unwrap().unwrap();

        assert_eq!(*actual, 26);
    }

    #[test]
    fn map_of_deque() {
        let store = Store::new(MapStore::new());
        let mut edit_map: Map<u32, Deque<u32>> = Map::create(store.clone(), ()).unwrap();

        edit_map
            .entry(42)
            .unwrap()
            .or_insert(Meta::default())
            .unwrap();

        let mut deque = edit_map.get_mut(42).unwrap().unwrap();
        deque.push_front(84).unwrap();

        edit_map.flush().unwrap();

        let mut read_map: Map<u32, Deque<u32>> = Map::create(store.clone(), ()).unwrap();
        let actual = read_map
            .get_mut(42)
            .unwrap()
            .unwrap()
            .pop_front()
            .unwrap()
            .unwrap();

        assert_eq!(*actual, 84);
    }
}
