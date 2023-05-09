use std::{any::Any, cell::RefCell, marker::PhantomData};

use crate::{
    encoding::{Decode, Encode},
    merk::BackingStore,
    query::Query,
    state::State,
    store::{
        bufstore::PartialMapStore, log::ReadLog, BufStore, Error as StoreError, Read, Shared,
        Store, Write, KV,
    },
    Error, Result,
};

use super::Client;

#[derive(Clone, Default)]
pub struct MockClient<T> {
    pub queries: RefCell<Vec<Vec<u8>>>,
    pub store: Store,
    _marker: PhantomData<T>,
}

impl<T: State + Query> Client for MockClient<T> {
    async fn query(&self, query: &[u8]) -> Result<Store> {
        self.queries.borrow_mut().push(query.to_vec());

        let store = Store::new(BackingStore::Other(Shared::new(Box::new(ReadLog::new(
            self.store.clone(),
        )))));
        let root_bytes = store.get(&[])?.unwrap_or_default();
        let app = T::load(store.clone(), &mut root_bytes.as_slice())?;
        let query = T::Query::decode(query)?;
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
        let map = out_store.into_map();
        let out_store = BufStore::wrap_with_map(crate::store::null::Unknown, map);

        Ok(Store::new(BackingStore::PartialMapStore(Shared::new(
            out_store,
        ))))
    }

    async fn call(&self, call: &[u8]) -> Result<()> {
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
