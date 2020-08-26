use std::borrow::Borrow;
use std::marker::PhantomData;

use crate::encoding::{Decode, Encode};
use crate::state::{State, Query};
use crate::store::{Read, Store, Entry};
use crate::Result;

/// A map data structure.
pub struct Map<S, K, V>
where
    S: Read,
    K: Encode + Decode,
    V: Encode + Decode,
{
    store: S,
    key_type: PhantomData<K>,
    value_type: PhantomData<V>,
}

impl<S, K, V> State<S> for Map<S, K, V>
where
    S: Read,
    K: Encode + Decode,
    V: Encode + Decode,
{
    /// Constructs a `Map` which is backed by the given store.
    fn wrap_store(store: S) -> Result<Self> {
        Ok(Self {
            store,
            key_type: PhantomData,
            value_type: PhantomData,
        })
    }
}

impl<S, K, V> Map<S, K, V>
where
    S: Read,
    K: Encode + Decode,
    V: Encode + Decode,
{
    /// Gets the value with the given key from the map, or `None` if there is no
    /// entry with the given key.
    ///
    /// If there is an error when getting from the store or decoding the value,
    /// the error will be returned.
    pub fn get<B: Borrow<K>>(&self, key: B) -> Result<Option<V>> {
        let (key_bytes, key_length) = encode_key_array(key.borrow())?;
        self.store
            .get(&key_bytes[..key_length])?
            .map(|value_bytes| V::decode(value_bytes.as_slice()))
            .transpose()
    }
}


impl<S, K, V> Query for Map<S, K, V>
where
    S: Read,
    K: Encode + Decode,
    V: Encode + Decode,
{
    type Request = K;
    type Response = Option<V>;

    fn query(&self, key: K) -> Result<Option<V>> {
        self.get(key)
    }

    fn resolve(&self, key: K) -> Result<()> {
        // TODO: make a get_raw method and use that instead
        let (key_bytes, key_length) = encode_key_array(key.borrow())?;
        self.store.get(&key_bytes[..key_length])?;
        Ok(())
    }
}

impl<S, K, V> Map<S, K, V>
where
    S: Store,
    K: Encode + Decode,
    V: Encode + Decode,
{
    /// Inserts the given key/value entry into the map. If an entry already
    /// exists with this key, it will be overwritten.
    ///
    /// If there is an error encoding the key or value, or when writing to the
    /// store, the error will be returned.
    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        let key_bytes = key.encode()?;
        let value_bytes = value.encode()?;
        self.store.put(key_bytes, value_bytes)
    }

    /// Deleted the entry with the given key.
    ///
    /// If there is an error encoding the key, or when deleting from the store,
    /// the error will be returned.
    pub fn delete<B: Borrow<K>>(&mut self, key: B) -> Result<()> {
        let (key_bytes, key_length) = encode_key_array(key.borrow())?;
        self.store.delete(&key_bytes[..key_length])
    }
}

impl<S, K, V> Map<S, K, V>
where
    S: Store + crate::store::Iter,
    K: Encode + Decode,
    V: Encode + Decode,
{
    /// Creates an iterator over the entries in the map, starting at the given
    /// key (inclusive).
    ///
    /// Iteration happens in bytewise order of encoded keys.
    pub fn iter_from(&self, start: &K) -> Result<Iter<'_, S::Iter<'_>, K, V>> {
        let start_bytes = start.encode()?;
        let iter = self.store.iter_from(start_bytes.as_slice());
        Ok(Iter::new(iter))
    }

    /// Creates an iterator over all the entries in the map.
    ///
    /// Iteration happens in bytewise order of encoded keys.
    pub fn iter(&self) -> Iter<'_, S::Iter<'_>, K, V> {
        let iter = self.store.iter();
        Iter::new(iter)
    }
}

pub struct Iter<'a, I, K, V>
where
    I: Iterator<Item = Entry<'a>>,
    K: Decode,
    V: Decode,
{
    iter: I,
    phantom_k: PhantomData<K>,
    phantom_v: PhantomData<V>,
}

impl<'a, I, K, V> Iter<'a, I, K, V>
where
    I: Iterator<Item = Entry<'a>>,
    K: Decode,
    V: Decode,
{
    /// Constructs an `Iter` for the given store.
    fn new(iter: I) -> Self {
        Iter {
            iter,
            phantom_k: PhantomData,
            phantom_v: PhantomData,
        }
    }
}

impl<'a, I, K, V> Iterator for Iter<'a, I, K, V>
where
    I: Iterator<Item = Entry<'a>>,
    K: Decode,
    V: Decode,
{
    type Item = (K, V);

    /// Gets the next entry from the map.
    ///
    /// This method will panic if decoding the entry fails.
    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()
            .map(|(key, value)| (K::decode(key).unwrap(), V::decode(value).unwrap()))
    }
}

/// Encodes the given key value into a fixed-size array. Returns the array and
/// the length of the encoded value.
///
/// This method will panic if the encoded key is longer than 256 bytes.
fn encode_key_array<K: Encode>(key: &K) -> Result<([u8; 256], usize)> {
    let mut bytes = [0; 256];
    key.encode_into(&mut &mut bytes[..])?;
    Ok((bytes, key.encoding_length()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::*;

    #[test]
    fn simple() {
        let mut store = MapStore::new();
        let mut map: Map<_, u64, u64> = Map::wrap_store(&mut store).unwrap();

        assert_eq!(map.get(1234).unwrap(), None);

        map.insert(1234, 5678).unwrap();
        assert_eq!(map.get(1234).unwrap(), Some(5678));

        map.delete(1234).unwrap();
        assert_eq!(map.get(1234).unwrap(), None);
    }

    #[test]
    fn iter() {
        let store = MapStore::new();
        let mut map: Map<_, u64, u64> = Map::wrap_store(store).unwrap();

        map.insert(123, 456).unwrap();
        map.insert(100, 100).unwrap();
        map.insert(400, 100).unwrap();

        let mut iter = map.iter_from(&101).unwrap();
        assert_eq!(iter.next(), Some((123, 456)));
        assert_eq!(iter.next(), Some((400, 100)));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn read_only() {
        let mut store = MapStore::new();
        let mut map: Map<_, u32, u32> = (&mut store).wrap().unwrap();
        map.insert(12, 34).unwrap();
        map.insert(56, 78).unwrap();

        let store = store;
        let map: Map<_, u32, u32> = store.wrap().unwrap();
        assert_eq!(map.get(12).unwrap(), Some(34));
        assert_eq!(map.get(56).unwrap(), Some(78));

        let collected = map.iter().collect::<Vec<(u32, u32)>>();
        assert_eq!(collected, vec![(12, 34), (56, 78)]);
    }

    #[test]
    fn query_resolve() {
        let mut store = RWLog::wrap(MapStore::new());
        let mut map: Map<_, u32, u32> = (&mut store).wrap().unwrap();
        map.insert(1, 2).unwrap();
        map.insert(3, 4).unwrap();

        map.resolve(1).unwrap(); // exists
        map.resolve(10).unwrap(); // doesn't exist
        
        let (reads, _, _) = store.finish();
        assert_eq!(reads.len(), 2);
        assert!(reads.contains(&[0, 0, 0, 1][..]));
        assert!(reads.contains(&[0, 0, 0, 10][..]));
    }

    #[test]
    fn query() {
        let mut store = MapStore::new();
        let mut map: Map<_, u32, u32> = (&mut store).wrap().unwrap();
        map.insert(1, 2).unwrap();
        map.insert(3, 4).unwrap();

        assert_eq!(map.query(1).unwrap(), Some(2));
        assert_eq!(map.query(10).unwrap(), None);
    }
}
