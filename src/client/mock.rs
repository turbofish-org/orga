use std::{cell::RefCell, marker::PhantomData};

use crate::{
    store::{Error as StoreError, Read, Store, Write, KV},
    Error, Result,
};

use super::Client;

#[derive(Clone, Default)]
pub struct MockClient<T> {
    pub queries: RefCell<Vec<Vec<u8>>>,
    pub store: Store,
    _marker: PhantomData<T>,
}

impl<T> Client for MockClient<T> {
    async fn query(&self, query: &[u8]) -> Result<Store> {
        self.queries.borrow_mut().push(query.to_vec());
        // TODO: copy keys accessed in query into a BufStore<UnknownStore>, return
        // Ok(self.store)
        Ok(Store::with_partial_map_store())
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
