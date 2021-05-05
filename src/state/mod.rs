use std::cell::{RefCell, Ref, RefMut};
use std::collections::{HashMap, BTreeSet};
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

pub struct MutStore<'a>(Vec<u8>, Shared<ReadWriter<'a>>);
impl<'a> MutStore<'a> {
    fn new<R: ReadWrite>(r: &'a mut R) -> Self {
        MutStore(vec![], Shared::new(ReadWriter(r)))
    }
}
impl<'a> Sub for MutStore<'a> {
    fn sub(&self, prefix: Vec<u8>) -> Self {
        MutStore(prefix, self.1.clone())
    }
}
impl<'a> Read2 for MutStore<'a> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut prefixed = self.0.clone();
        prefixed.extend_from_slice(key);
        Read2::get(&self.1, prefixed.as_slice())
    }
}
impl<'a> Write2 for MutStore<'a> {
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

    fn flush(self) -> Result<()>
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
    use std::{borrow::{Borrow, BorrowMut}, marker::PhantomData};

    use super::*;
    use crate::store::split::Splitter2;
    use crate::store::*;
    use ed::*;

    #[derive(Clone)]
    struct Child<'a, K, V: State2<S>, S> {
        key_bytes: Vec<u8>,
        value: Rc<RefCell<V>>,
        parent: &'a Map<'a, K, V, S>,
    }

    impl<'a, K, V: State2<S>, S> Child<'a, K, V, S> {
        pub fn borrow(&self) -> Ref<'_, V> {
            self.value.deref().borrow()
        }

        pub fn borrow_mut(&mut self) -> RefMut<'_, V> {
            self.parent.pending_writes
                .borrow_mut()
                .insert(self.key_bytes.clone());
            self.value.deref().borrow_mut()
        }
    }

    struct Map<'a, K, V: State2<S>, S: 'a = MutStore<'a>> {
        store: S,
        // XXX: should we be using the key type and requiring Hash or encoding
        // the keys into bytes?
        children: RefCell<HashMap<Vec<u8>, Rc<RefCell<V>>>>,
        pending_writes: RefCell<BTreeSet<Vec<u8>>>,
        marker: PhantomData<&'a K>,
    }

    impl<'a, K, V: State2<S>, S> State2<S> for Map<'a, K, V, S> {
        type Encoding = ();

        fn create(store: S, _: ()) -> crate::Result<Self> {
            Ok(Map {
                store,
                children: Default::default(),
                pending_writes: Default::default(),
                marker: PhantomData,
            })
        }

        fn flush(self) -> crate::Result<()>
        where
            S: Write2,
        {
            todo!()
        }
    }

    impl<'a, K, V: State2<S>, S> From<Map<'a, K, V, S>> for () {
        fn from(_: Map<K, V, S>) -> Self {
            ()
        }
    }

    impl<'a, K: Encode, V: State2<S>, S: Read2 + Sub + 'a> Map<'a, K, V, S> {
        fn get(&self, key: &K) -> crate::Result<Option<Child<K, V, S>>> {
            let key_bytes = key.encode()?;
            if let Some(value) = self.children.borrow().get(key_bytes.as_slice()) {
                return Ok(Some(Child {
                    key_bytes: key_bytes.clone(),
                    value: value.clone(),
                    parent: self,
                }))
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
                None => None,
                Some(bytes) => {
                    let decoded = V::Encoding::decode(bytes.as_slice())?;
                    let child = self.create_child(key_bytes, decoded)?;
                    Some(child)
                }
            })
        }

        fn create_child(
            &self,
            key_bytes: Vec<u8>,
            value: V::Encoding,
        ) -> crate::Result<Child<K, V, S>> {
            let value = Rc::new(RefCell::new(
                V::create(self.store.sub(key_bytes.clone()), value)?
            ));

            self.children.borrow_mut().insert(key_bytes.clone(), value.clone());

            Ok(Child {
                key_bytes,
                value,
                parent: self,
            })
        }

        fn get_or_create(&mut self, key: &K, default: V::Encoding) -> crate::Result<Child<K, V, S>> {
            Ok(match self.get(key)? {
                Some(value) => value,
                None => {
                    let key_bytes = key.encode()?;
                    self.create_child(key_bytes, default)?
                },
            })
        }
    }

    impl<'a, K, V, S, D> Map<'a, K, V, S>
    where
        K: Encode,
        V: State2<S, Encoding = D>,
        S: Read2 + Sub + 'a,
        D: Default,
    {
        fn get_or_default(&mut self, key: &K) -> crate::Result<Child<K, V, S>> {
            self.get_or_create(key, Default::default())
        }
    }

    impl<'a, K, V, S> Map<'a, K, V, S>
    where
        K: Encode,
        V: State2<S>,
        S: Read2 + Write2 + Sub + 'a,
    {
        fn insert(&mut self, key: K, value: V) -> crate::Result<()> {
            let key_bytes = key.encode()?;
            let encoding_value: V::Encoding = value.into();
            let value_bytes = encoding_value.encode()?;
            self.store.put(key_bytes, value_bytes)
            // TODO: insert into children, mark for write
        }
    }

    struct CountedMap<'a, K: State2<S>, V: State2<S>, S: Read2> {
        count: u64,
        map: Map<'a, K, V, S>,
    }

    impl<'a, K: State2<S>, V: State2<S>, S: Read2> CountedMap<'a, K, V, S> {
        // fn
    }

    type CountedMapEncoding<'a, K: State2<S>, V: State2<S>, S: Read2 + Sub> = (
        <u64 as State2<S>>::Encoding,
        <Map<'a, K, V, S> as State2<S>>::Encoding,
    );

    impl<'a, K, V, S> State2<S> for CountedMap<'a, K, V, S>
    where
        K: State2<S>,
        V: State2<S>,
        S: Read2 + Sub,
    {
        type Encoding = CountedMapEncoding<'a, K, V, S>;

        fn create(store: S, decoded: Self::Encoding) -> crate::Result<Self> {
            Ok(Self {
                count: State2::create(store.sub(vec![0]), decoded.0)?,
                map: State2::create(store.sub(vec![1]), decoded.1)?,
            })
        }

        fn flush(self) -> crate::Result<()>
        where
            S: Write2,
        {
            todo!()
        }
    }

    impl<'a, K: State2<S>, V: State2<S>, S: Read2> From<CountedMap<'a, K, V, S>>
        for CountedMapEncoding<'a, K, V, S>
    {
        fn from(state: CountedMap<'a, K, V, S>) -> Self {
            (state.count.into(), state.map.into())
        }
    }

    #[cfg(test)]
    #[test]
    fn state2() {
        let mut store = MapStore::new();

        let mut map: Map<u64, Map<u64, u64>> =
            Map::create(MutStore::new(&mut store), ()).unwrap();

        let mut submap = map.get_or_default(&123).unwrap();
        
        submap
            .borrow_mut()
            .insert(45, 67).unwrap();

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
