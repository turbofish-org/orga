use std::borrow::Borrow;
use std::collections::{hash_map, HashMap};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

use crate::state::*;
use crate::store::*;
use crate::Result;
use ed::*;

pub struct Map<K, V, S = Store> {
    store: S,
    children: HashMap<K, V>,
}

impl<K, V, S> State<S> for Map<K, V, S>
where
    K: Encode + Eq + Hash,
    V: State<S>,
    S: Write + Sub,
{
    type Encoding = ();

    fn create(store: S, _: ()) -> Result<Self> {
        Ok(Map {
            store,
            children: Default::default(),
        })
    }

    fn flush(mut self) -> Result<()> {
        for (key, value) in self.children.drain() {
            let key_bytes = key.encode()?;
            let value_bytes = value.flush()?.encode()?;
            self.store.put(key_bytes, value_bytes)?;
        }

        Ok(())
    }
}

impl<K, V, S> From<Map<K, V, S>> for () {
    fn from(_: Map<K, V, S>) -> Self {
        ()
    }
}

impl<K, V, S> Map<K, V, S>
where
    K: Encode + Terminated + Eq + Hash,
    V: State<S>,
    S: Read + Sub,
{
    pub fn get<B: Borrow<K>>(&self, key: B) -> Result<Option<Child<V>>> {
        let key = key.borrow();
        Ok(if self.children.contains_key(key) {
            // value is already retained in memory (was modified)
            let child = self.children.get(key).unwrap();
            let child = Child::Modified(child);
            Some(child)
        } else {
            // value is not in memory, try to get from store
            self.get_from_store(key)?
                .map(Child::Unmodified)
        }) 
    }

    pub fn get_mut(&mut self, key: K) -> Result<Option<ChildMut<K, V, S>>> {
        Ok(self.entry(key)?.into())
    }

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
                },
                None => Entry::Vacant { key, parent: self },
            }
        })
    }

    fn get_from_store(&self, key: &K) -> Result<Option<V>> {
        let key_bytes = key.encode()?;
        self.store
            .get(key_bytes.as_slice())?
            .map(|value_bytes| {
                let substore = self.store.sub(key_bytes);
                let decoded = V::Encoding::decode(value_bytes.as_slice())?;
                V::create(substore, decoded)
            })
            .transpose()
    }
}

pub enum Child<'a, V> {
    Unmodified(V),
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

pub enum Entry<'a, K, V, S> {
    Vacant {
        key: K,
        parent: &'a mut Map<K, V, S>,
    },
    Occupied {
        child: ChildMut<'a, K, V, S>,
    },
}

impl<'a, K, V, S> Entry<'a, K, V, S>
where
    K: Encode + Terminated + Eq + Hash,
    V: State<S>,
    S: Read + Sub,
{
    pub fn or_create(self, value: V::Encoding) -> Result<ChildMut<'a, K, V, S>> {
        Ok(match self {
            Entry::Vacant { key, parent } => {
                let key_bytes = key.encode()?;
                let substore = parent.store.sub(key_bytes);
                let value = V::create(substore, value)?;
                ChildMut::Unmodified(Some((key, value, parent)))
            }
            Entry::Occupied { child } => child,
        })
    }

    pub fn or_insert(self, value: V::Encoding) -> Result<ChildMut<'a, K, V, S>> {
        let mut child = self.or_create(value)?;
        child.deref_mut();
        Ok(child)
    }
}

impl<'a, K, V, S, D> Entry<'a, K, V, S>
where
    K: Encode + Terminated + Eq + Hash,
    V: State<S, Encoding = D>,
    S: Read + Sub,
    D: Default,
{
    pub fn or_default(self) -> Result<ChildMut<'a, K, V, S>> {
        self.or_create(D::default())
    }

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

pub enum ChildMut<'a, K, V, S> {
    Unmodified(Option<(K, V, &'a mut Map<K, V, S>)>),
    Modified(hash_map::OccupiedEntry<'a, K, V>),
}

impl<'a, K, V, S> Deref for ChildMut<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        match self {
            ChildMut::Unmodified(inner) => &inner.as_ref().unwrap().1,
            ChildMut::Modified(entry) => entry.get(),
        }
    }
}

impl<'a, K, V, S> DerefMut for ChildMut<'a, K, V, S>
where
    K: Eq + Hash
{
    fn deref_mut(&mut self) -> &mut V {
        match self {
            ChildMut::Unmodified(inner) => {
                // insert into parent's children map and upgrade child to
                // Child::ModifiedMut
                let (key, value, parent) = inner.take().unwrap();
                let entry = parent.children
                    .entry(key)
                    .insert(value);
                *self = ChildMut::Modified(entry);
                self.deref_mut()
            },
            ChildMut::Modified(entry) => entry.get_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Store;
    use crate::store::MapStore;

    fn increment_entry(map: &mut Map<u64, u64>, n: u64) -> Result<()> {
        *map.entry(n)?.or_default()? += 1;
        Ok(())
    }

    #[test]
    fn submap() {
        let mapstore = Shared::new(MapStore::new());
        let store = Store::new(Box::new(mapstore.clone()));
        let mut map: Map<u64, Map<u64, u64>> = Map::create(store, ()).unwrap();

        let mut submap = map
            .entry(123).unwrap()
            .or_default().unwrap();
        increment_entry(&mut submap, 456).unwrap();

        let map_ref = &map;
        assert_eq!(
            *map_ref
                .get(123).unwrap().unwrap()
                .get(456).unwrap().unwrap(),
            1
        );

        map.flush().unwrap();

        for (key, value) in mapstore.iter() {
            println!("{:?}: {:?}", key, value);
        }
    }
}
