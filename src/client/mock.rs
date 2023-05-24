use std::{any::Any, cell::RefCell, marker::PhantomData};

use ed::Terminated;

use crate::{
    call::Call,
    encoding::{Decode, Encode},
    merk::BackingStore,
    plugins::QueryPlugin,
    query::Query,
    state::State,
    store::{
        bufstore::PartialMapStore, log::ReadLog, BufStore, Error as StoreError, Read, Shared,
        Store, Write, KV,
    },
    Error, Result,
};

use super::exec::Client;

#[derive(Clone, Default)]
pub struct MockClient<T> {
    pub queries: RefCell<Vec<Vec<u8>>>,
    pub store: Store,
    _marker: PhantomData<T>,
}

impl<T> MockClient<T> {
    pub fn with_store(store: Store) -> Self {
        Self {
            queries: RefCell::new(vec![]),
            store,
            _marker: PhantomData,
        }
    }
}

impl<T: State + Query + Call> Client<QueryPlugin<T>> for MockClient<QueryPlugin<T>> {
    async fn query(&self, query: &<QueryPlugin<T> as Query>::Query) -> Result<Store> {
        let query_bytes = query.encode()?;
        self.queries.borrow_mut().push(query_bytes.clone());

        let store = Store::new(BackingStore::Other(Shared::new(Box::new(ReadLog::new(
            self.store.clone(),
        )))));
        let root_bytes = store.get(&[])?.unwrap_or_default();
        let app = QueryPlugin::<T>::load(store.clone(), &mut root_bytes.as_slice())?;
        let query = <QueryPlugin<_> as Query>::Query::decode(query_bytes.as_slice())?;
        app.query(query)?;
        drop(app);

        let log = if let BackingStore::Other(b) = store.into_backing_store().into_inner() {
            let b = b.into_inner() as Box<dyn Any>;
            b.downcast::<ReadLog<Store>>().unwrap().reads().clone()
        } else {
            unreachable!()
        };
        let mut out_store = PartialMapStore::new();
        for key in log {
            match self.store.get(&key)? {
                Some(value) => out_store.put(key, value)?,
                None => out_store.delete(&key)?,
            }
        }
        // let map = out_store.into_map();
        // let out_store = BufStore::wrap_with_map(crate::store::null::Unknown, map);

        Ok(Store::new(BackingStore::PartialMapStore(Shared::new(
            out_store,
        ))))
    }

    async fn call(&self, call: &<QueryPlugin<T> as Call>::Call) -> Result<()> {
        todo!()
    }
}

#[derive(Default, Clone)]
struct UnknownStore;

impl Read for UnknownStore {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Err(Error::StoreErr(StoreError::ReadUnknown(key.to_vec())))
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        Ok(None)
    }
}

impl Write for UnknownStore {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        // TODO: WriteUnknown error
        unimplemented!()
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        // TODO: WriteUnknown error
        unimplemented!()
    }
}
