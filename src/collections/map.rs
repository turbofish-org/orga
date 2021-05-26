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

    fn create(store: S, _: ()) -> Result<Self>
    where
        S: Read,
    {
        Ok(Map {
            store,
            children: Default::default(),
        })
    }

    fn flush(mut self) -> Result<()>
    where
        S: Write,
    {
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
    K: Encode + Eq + Hash,
    V: State<S>,
    S: Read + Sub,
{
    pub fn entry(&mut self, key: K) -> Result<Entry<K, V, S>> {
        Ok(if self.children.contains_key(&key) {
            // value is already loaded
            let entry = self.children.entry(key);
            let entry = match entry {
                hash_map::Entry::Occupied(entry) => entry,
                _ => unreachable!(),
            };
            Entry::Occupied { entry }
        } else {
            // value is not loaded
            let key_bytes = key.encode()?;
            if let Some(value_bytes) = self.store.get(key_bytes.as_slice())? {
                // value exists in store, create instance and put it in map of
                // children
                let substore = self.store.sub(key_bytes);
                let decoded = V::Encoding::decode(value_bytes.as_slice())?;
                let value = V::create(substore, decoded)?;

                let entry = self.children
                    .entry(key)
                    .insert(value);

                Entry::Occupied { entry }
            } else {
                // value doesn't exist in store
                Entry::Vacant { key, parent: self }
            }
        })
    }
}

pub enum Entry<'a, K, V, S> {
    Vacant {
        key: K,
        parent: &'a mut Map<K, V, S>,
    },
    Occupied {
        entry: hash_map::OccupiedEntry<'a, K, V>,
    },
}

impl<'a, K, V, S> Entry<'a, K, V, S>
where
    K: Encode + Eq + Hash,
    V: State<S>,
    S: Read + Sub,
{
    pub fn or_create(self, value: V::Encoding) -> Result<Child<'a, K, V, S>> {
        Ok(match self {
            Entry::Vacant { key, parent } => {
                let key_bytes = key.encode()?;
                let substore = parent.store.sub(key_bytes);
                let value = V::create(substore, value)?;
                Child::Unmodified(Some((key, value, parent)))
            }
            Entry::Occupied { entry } => Child::Modified(entry),
        })
    }

    pub fn or_insert(self, value: V::Encoding) -> Result<Child<'a, K, V, S>> {
        let mut child = self.or_create(value)?;
        child.deref_mut();
        Ok(child)
    }
}

impl<'a, K, V, S, D> Entry<'a, K, V, S>
where
    K: Encode + Eq + Hash,
    V: State<S, Encoding = D>,
    S: Read + Sub,
    D: Default,
{
    pub fn or_default(self) -> Result<Child<'a, K, V, S>> {
        self.or_create(D::default())
    }

    pub fn or_insert_default(self) -> Result<Child<'a, K, V, S>> {
        self.or_insert(D::default())
    }
}

pub enum Child<'a, K, V, S> {
    Unmodified(Option<(K, V, &'a mut Map<K, V, S>)>),
    Modified(hash_map::OccupiedEntry<'a, K, V>),
}

impl<'a, K, V, S> Deref for Child<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &V {
        match self {
            Child::Unmodified(inner) => &inner.as_ref().unwrap().1,
            Child::Modified(entry) => entry.get(),
        }
    }
}

impl<'a, K, V, S> DerefMut for Child<'a, K, V, S>
where
    K: Eq + Hash
{
    fn deref_mut(&mut self) -> &mut V {
        match self {
            Child::Unmodified(inner) => {
                // insert into parent's children map and upgrade child to
                // Child::Modified
                let (key, value, parent) = inner.take().unwrap();
                let entry = parent.children
                    .entry(key)
                    .insert(value);
                *self = Child::Modified(entry);
                self.deref_mut()
            },
            Child::Modified(entry) => entry.get_mut(),
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
        let mut map: Map<u64, Map<u64, u64>, _> = Map::create(store, ()).unwrap();

        let mut submap = map
            .entry(123).unwrap()
            .or_default().unwrap();
        increment_entry(&mut submap, 456).unwrap();

        map.flush().unwrap();

        for (key, value) in mapstore.iter() {
            println!("{:?}: {:?}", key, value);
        }
    }
}
