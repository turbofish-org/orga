use std::borrow::Borrow;
use std::collections::{hash_map, HashMap};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

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
    children: HashMap<K, Option<V>>,
}

impl<K, V, S> From<Map<K, V, S>> for () {
    fn from(_map: Map<K, V, S>) {}
}

impl<K, V, S> State<S> for Map<K, V, S>
where
    K: Encode + Terminated + Eq + Hash,
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
        for (key, maybe_value) in self.children.drain() {
            Self::apply_change(&mut self.store, &key, maybe_value)?;
        }

        Ok(())
    }
}

impl<K, V, S> Map<K, V, S>
where
    K: Encode + Terminated + Eq + Hash,
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
                hash_map::Entry::Occupied(entry) => entry,
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
    Modified(hash_map::OccupiedEntry<'a, K, Option<V>>),
}

impl<'a, K, V, S> ChildMut<'a, K, V, S>
where
    K: Hash + Eq + Encode + Terminated,
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

impl<'a, K, V, S> Deref for ChildMut<'a, K, V, S> {
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
    K: Eq + Hash,
{
    fn deref_mut(&mut self) -> &mut V {
        match self {
            ChildMut::Unmodified(inner) => {
                // insert into parent's children map and upgrade child to
                // Child::ModifiedMut
                let (key, value, parent) = inner.take().unwrap();
                let entry = parent.children.entry(key).insert(Some(value));
                *self = ChildMut::Modified(entry);
                self.deref_mut()
            }
            ChildMut::Modified(entry) => entry.get_mut().as_mut().unwrap(),
        }
    }
}

/// A mutable reference to a key/value entry in a collection, which may be
/// empty.
pub enum Entry<'a, K, V, S> {
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
    K: Encode + Terminated + Eq + Hash,
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
    K: Encode + Terminated + Eq + Hash,
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
    K: Encode + Terminated + Eq + Hash,
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

impl<'a, K, V, S> From<Entry<'a, K, V, S>> for Option<ChildMut<'a, K, V, S>> {
    fn from(entry: Entry<'a, K, V, S>) -> Self {
        match entry {
            Entry::Vacant { .. } => None,
            Entry::Occupied { child } => Some(child),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MapStore, Store};

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

    fn enc(n: u32) -> Vec<u8> {
        n.encode().unwrap()
    }
}
