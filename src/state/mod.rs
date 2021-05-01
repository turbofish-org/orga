use std::ops::{Deref, DerefMut};

use crate::Result;
use crate::store::*;

pub mod value;
pub mod wrapper;

pub use value::Value;
pub use wrapper::WrapperStore;

pub struct Store2 (Shared<Box<dyn Read2>>);
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

pub struct MutStore (Box<dyn Write2>);
impl MutStore {
    fn new<R: Write2 + 'static>(r: R) -> Self {
        MutStore(Box::new(r))
    }
}
impl Read2 for MutStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.0.get(key)
    }
}
impl Write2 for MutStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.0.put(key, value)
    }
   
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.0.delete(key)
    }
}

pub trait State2<S>: Sized {
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    fn create(store: S, decoded: Self::Encoding) -> Result<Self>
        where S: Read2;

    fn destroy(self) -> Result<()>
        where S: Write2;
}

impl<S> State2<S> for u64 {
    type Encoding = Self;

    fn create(_: S, value: Self) -> Result<Self> {
        Ok(value)
    }

    fn destroy(self) -> Result<()>
    where S: Write2 {
        Ok(())
    }
}
// impl<S: Write2> MutState<S> for u64 {}

impl<S> State2<S> for u8 {
    type Encoding = Self;

    fn create(_: S, value: Self) -> Result<Self> {
        Ok(value)
    }

    fn destroy(self) -> Result<()>
    where S: Write2 {
        Ok(())
    }
}
// impl<S: Write2> MutState<S> for u8 {}

mod tests2 {
    use super::*;
    use crate::store::*;
    use crate::store::split::Splitter2;
    use ed::*;

    struct Value<T> {
        value: std::rc::Rc<std::cell::RefCell<T>>,
        // parent: &'a Map<
    }

    struct Map<K, V: State2<S>, S = Store2> {
        store: S,
        marker: std::marker::PhantomData<(K, V)>,
        children: std::collections::HashMap<K, V>,
        // pending_writes: std::collections::HashSet<
    }

    impl<K, V: State2<S>, S: Read2> State2<S> for Map<K, V, S> {
        type Encoding = ();

        fn create(store: S, _: ()) -> crate::Result<Self> {
            Ok(Map {
                store,
                marker: std::marker::PhantomData,
                children: Default::default(),
            })
        }

        fn destroy(self) -> crate::Result<()>
        where S: Write2 {
            todo!()
        }
    }
    
    impl<K, V: State2<S>, S> From<Map<K, V, S>> for () {
        fn from(_: Map<K, V, S>) -> Self {
            ()
        }
    }

    impl<K: Encode, V: State2<S>, S: Read2 + Sub + 'static> Map<K, V, S> {
        fn get(&self, key: K) -> crate::Result<Option<V>> {
            // TODO: we shouldn't have to get if encoding is () 🤔 - can prevent
            // by creating a io::Read implementation which fetches a value for a
            // given key only on first read
            let key_bytes = key.encode()?;
            let value_bytes = self.store
                .get(key_bytes.as_slice())?;
            Ok(match value_bytes {
                None => None,
                Some(bytes) => {
                    let value = V::Encoding::decode(bytes.as_slice())?;
                    Some(V::create(self.store.sub(key_bytes), value)?)
                }
            })
        }

        fn get_or_create(&mut self, key: K, default: V::Encoding) -> crate::Result<V> {
            let key_bytes = key.encode()?;
            Ok(match self.get(key)? {
                Some(value) => value,
                None => V::create(self.store.sub(key_bytes), default)?
            })
        }
    }

    impl<K: Encode, V: State2<S, Encoding = D>, S: Read2 + Sub + 'static, D: Default> Map<K, V, S> {
        fn get_or_default(&mut self, key: K) -> crate::Result<V> {
            self.get_or_create(key, Default::default())
        }
    }

    impl<K: Encode, V: State2<S>, S: Read2 + Write2 + Sub + 'static> Map<K, V, S> {
        fn insert(&mut self, key: K, value: V) -> crate::Result<()> {
            let key_bytes = key.encode()?;
            let encoding_value: V::Encoding = value.into();
            let value_bytes = encoding_value.encode()?;
            self.store.put(key_bytes, value_bytes)
            // TODO: insert into children, mark for write
        }
    }

    struct CountedMap<K: State2<S>, V: State2<S>, S: Read2> {
        count: u64,
        map: Map<K, V, S>,
    }

    impl<K: State2<S>, V: State2<S>, S: Read2> CountedMap<K, V, S> {
        // fn 
    }

    type CountedMapEncoding<K: State2<S>, V: State2<S>, S: Read2 + Sub> = (
        <u64 as State2<S>>::Encoding,
        <Map<K, V, S> as State2<S>>::Encoding,
    );
    
    impl<K: State2<S>, V: State2<S>, S: Read2 + Sub> State2<S> for CountedMap<K, V, S> {
        type Encoding = CountedMapEncoding<K, V, S>;

        fn create(store: S, decoded: Self::Encoding) -> crate::Result<Self> {
            Ok(Self {
                count: State2::create(store.sub(vec![0]), decoded.0)?,
                map: State2::create(store.sub(vec![1]), decoded.1)?,
            })
        }

        fn destroy(self) -> crate::Result<()>
        where S: Write2 {
            todo!()
        }
    }

    impl<K: State2<S>, V: State2<S>, S: Read2> From<CountedMap<K, V, S>> for CountedMapEncoding<K, V, S> {
        fn from(state: CountedMap<K, V, S>) -> Self {
            (
                state.count.into(),
                state.map.into(),
            )
        }
    }

    #[cfg(test)]
    fn state2() {
        let store = MapStore::new();


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
