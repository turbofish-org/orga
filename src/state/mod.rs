use std::cell::{Ref, RefCell, RefMut};
use std::collections::{HashSet, HashMap};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::store::*;
use crate::Result;

pub mod value;
pub mod wrapper;

pub use value::Value;
pub use wrapper::WrapperStore;

pub struct Store2(Shared<Box<dyn Read2>>);
impl Store2 {
    fn new<R: Read2 + 'static>(r: R) -> Self {
        Store2(Shared::new(Box::new(r)))
    }
}
impl Sub for Store2 {
    fn sub(&self, prefix: Vec<u8>) -> Self {
        Store2(self.0.clone())
    }
}
impl Deref for Store2 {
    type Target = Shared<Box<dyn Read2>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct MutStore(Vec<u8>, Shared<ReadWriter>);
impl<'a> MutStore {
    fn new(r: Box<dyn ReadWrite>) -> Self {
        MutStore(vec![], Shared::new(ReadWriter(r)))
    }
}
impl<'a> Sub for MutStore {
    fn sub(&self, prefix: Vec<u8>) -> Self {
        MutStore(prefix, self.1.clone())
    }
}
impl<'a> Read2 for MutStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut prefixed = self.0.clone();
        prefixed.extend_from_slice(key);
        Read2::get(&self.1, prefixed.as_slice())
    }
}
impl<'a> Write2 for MutStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let mut prefixed = self.0.clone();
        prefixed.extend(key);
        Write2::put(&mut self.1, prefixed, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let mut prefixed = self.0.clone();
        prefixed.extend_from_slice(key);
        Write2::delete(&mut self.1, prefixed.as_slice())
    }
}

pub trait State2<S>: Sized {
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    fn create(store: S, decoded: Self::Encoding) -> Result<Self>
    where
        S: Read2;

    fn flush(&mut self) -> Result<()>
    where
        S: Write2,
    {
        Ok(())
    }
}

impl<S> State2<S> for u64 {
    type Encoding = Self;

    fn create(_: S, value: Self) -> Result<Self> {
        Ok(value)
    }
}

impl<S> State2<S> for u8 {
    type Encoding = Self;

    fn create(_: S, value: Self) -> Result<Self> {
        Ok(value)
    }
}

mod tests2 {
    use std::{
        borrow::{Borrow, BorrowMut},
        marker::PhantomData,
    };

    use super::*;
    use crate::store::split::Splitter2;
    use crate::store::*;
    use ed::*;

    enum Entry<'a, K, V: State2<S>, S> {
        Some(Child<'a, K, V, S>),
        None(Vec<u8>, &'a mut Map<K, V, S>),
    }

    impl<'a, K, V, S> Entry<'a, K, V, S>
    where
        V: State2<S>,
        S: Read2 + Sub,
    {
        pub fn or_create(self, value: V::Encoding) -> crate::Result<Child<'a, K, V, S>> {
            match self {
                Entry::Some(child) => Ok(child),
                Entry::None(key_bytes, parent) => parent.create_child(key_bytes, value),
            }
        }
    }

    impl<'a, K, V, D, S> Entry<'a, K, V, S>
    where
        V: State2<S, Encoding = D>,
        D: Default,
        S: Read2 + Sub,
    {
        pub fn or_default(self) -> crate::Result<Child<'a, K, V, S>> {
            self.or_create(Default::default())
        }
    }

    struct Child<'a, K, V: State2<S>, S> {
        key_bytes: Vec<u8>,
        parent: &'a mut Map<K, V, S>,
        modified: bool,
    }

    impl<'a, K, V: State2<S>, S> Deref for Child<'a, K, V, S> {
        type Target = V;

        fn deref(&self) -> &Self::Target {
            self.parent.children.get(&self.key_bytes).unwrap()
        }
    }

    impl<'a, K, V: State2<S>, S> DerefMut for Child<'a, K, V, S> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            if !self.modified {
                self.parent.pending_changes.insert(self.key_bytes.clone());
                self.modified = true;
            }

            self.parent.children.get_mut(&self.key_bytes).unwrap()
        }
    }

    struct Map<K, V: State2<S>, S = MutStore> {
        store: S,
        children: HashMap<Vec<u8>, V>,
        pending_changes: HashSet<Vec<u8>>,
        marker: PhantomData<K>,
    }

    impl<'a, K, V: State2<S>, S> State2<S> for Map<K, V, S> {
        type Encoding = ();

        fn create(store: S, _: ()) -> crate::Result<Self> {
            Ok(Map {
                store,
                children: Default::default(),
                pending_changes: Default::default(),
                marker: PhantomData,
            })
        }

        fn flush(&mut self) -> crate::Result<()>
        where
            S: Write2,
        {
            for key_bytes in self.pending_changes.drain() {
                let mut value = self.children.remove(&key_bytes).unwrap();
                value.flush()?;

                let encoding: V::Encoding = value.into();
                let value_bytes = encoding.encode()?;

                if value_bytes.len() > 0 {
                    self.store.put(key_bytes, value_bytes)?;
                }
            }

            Ok(())
        }
    }

    impl<'a, K, V: State2<S>, S> From<Map<K, V, S>> for () {
        fn from(_: Map<K, V, S>) -> Self {
            ()
        }
    }

    impl<'a, K: Encode, V: State2<S>, S: Read2 + Sub> Map<K, V, S> {
        pub fn entry(&'a mut self, key: &K) -> crate::Result<Entry<K, V, S>> {
            let key_bytes = key.encode()?;

            if self.children.contains_key(&key_bytes) {
                let child = self.child(key_bytes);
                return Ok(Entry::Some(child));
            }

            // TODO: we shouldn't have to get if encoding is () 🤔 - can prevent
            // by creating a io::Read implementation which fetches a value for a
            // given key only on first read. encodings which don't read any
            // bytes will never fail and so always have a valid instance, even
            // if never used before. if the encoding attempts to read but there
            // is no entry, the decoder can emit a special error (bubbled up
            // from the reader) which this function can handle by returning None
            let value_bytes = self.store.get(key_bytes.as_slice())?;
            Ok(match value_bytes {
                None => Entry::None(key_bytes, self),
                Some(bytes) => {
                    let decoded = V::Encoding::decode(bytes.as_slice())?;
                    Entry::Some(self.create_child(key_bytes, decoded)?)
                }
            })
        }
    }

    impl<'a, K, V: State2<S>, S: Read2 + Sub + 'a> Map<K, V, S> {
        fn create_child(
            &'a mut self,
            key_bytes: Vec<u8>,
            value: V::Encoding,
        ) -> crate::Result<Child<K, V, S>> {
            let substore = self.store.sub(key_bytes.clone());
            let value = V::create(substore, value)?;
            self.children.insert(key_bytes.clone(), value);
            Ok(self.child(key_bytes))
        }

        fn child(&'a mut self, key_bytes: Vec<u8>) -> Child<K, V, S> {
            Child {
                key_bytes,
                parent: self,
                modified: false,
            }
        }
    }

    // struct CountedMap<'a, K: State2<S>, V: State2<S>, S: Read2> {
    //     count: u64,
    //     map: Map<'a, K, V, S>,
    // }

    // impl<'a, K: State2<S>, V: State2<S>, S: Read2> CountedMap<'a, K, V, S> {
    //     // fn
    // }

    // type CountedMapEncoding<'a, K: State2<S>, V: State2<S>, S: Read2 + Sub> = (
    //     <u64 as State2<S>>::Encoding,
    //     <Map<'a, K, V, S> as State2<S>>::Encoding,
    // );

    // impl<'a, K, V, S> State2<S> for CountedMap<'a, K, V, S>
    // where
    //     K: State2<S>,
    //     V: State2<S>,
    //     S: Read2 + Sub,
    // {
    //     type Encoding = CountedMapEncoding<'a, K, V, S>;

    //     fn create(store: S, decoded: Self::Encoding) -> crate::Result<Self> {
    //         Ok(Self {
    //             count: State2::create(store.sub(vec![0]), decoded.0)?,
    //             map: State2::create(store.sub(vec![1]), decoded.1)?,
    //         })
    //     }

    //     fn flush(&mut self) -> crate::Result<()>
    //     where
    //         S: Write2,
    //     {
    //         todo!()
    //     }
    // }

    // impl<'a, K: State2<S>, V: State2<S>, S: Read2> From<CountedMap<'a, K, V, S>>
    //     for CountedMapEncoding<'a, K, V, S>
    // {
    //     fn from(state: CountedMap<'a, K, V, S>) -> Self {
    //         (state.count.into(), state.map.into())
    //     }
    // }

    #[derive(Encode, Decode, Default)]
    struct Account {
        balance: u64,
    }

    impl<S> State2<S> for Account {
        type Encoding = Self;

        fn create(_: S, value: Self) -> crate::Result<Self> {
            Ok(value)
        }
    }

    fn add_to_account(accounts: &mut Map<u64, Account>, address: u64, amount: u64) -> crate::Result<()> {
        let mut account = accounts
            .entry(&address)?
            .or_default()?;

        account.balance += amount;

        Ok(())
    }

    #[cfg(test)]
    #[test]
    fn state2() {
        let store = Shared::new(MapStore::new());

        let mut map: Map<u64, Map<u64, Account>> = Map::create(MutStore::new(Box::new(store.clone())), ()).unwrap();

        let mut submap = map
            .entry(&123).unwrap()
            .or_default().unwrap();

        add_to_account(&mut submap, 45, 67).unwrap();

        map.flush().unwrap();

        for (k, v) in store.iter() {
            println!("{:?}: {:?}", k, v);
        }
    }
}

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
pub trait State<S: Read>: Sized {
    fn wrap_store(store: S) -> Result<Self>;
}

/// A trait for state types that can have their data queried by a client.
///
/// A `Query` implementation will typically just call existing getter methods,
/// with the trait acting as a generic way to call these methods.
pub trait Query {
    /// The type of value sent from the client to the node which is resolving
    /// the query.
    type Request;

    /// The type of value returned to the client when a query is successfully
    /// resolved.
    type Response;

    /// Gets data from the state based on the incoming request, and returns it.
    ///
    /// This will be called client-side in order to reproduce the state access
    /// in order for the client to fully verify the data.
    fn query(&self, req: Self::Request) -> Result<Self::Response>;

    /// Accesses the underlying store to get the data necessary for the incoming
    /// query.
    ///
    /// This is called on the resolving node in order to know which raw store
    /// data to send back to the client to let the client successfully call
    /// `query`, using an instrumented store type which records which keys are
    /// accessed.
    ///
    /// The default implementation for `resolve` is to simply call `query` and
    /// throw away the response for ease of implementation, but this will
    /// typically mean unnecessary decoding the result type. Implementations may
    /// override `resolve` to more efficiently query the state without the extra
    /// decode step.
    fn resolve(&self, req: Self::Request) -> Result<()> {
        self.query(req)?;
        Ok(())
    }
}
