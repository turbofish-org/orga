//! Mock client for use in tests.
use std::{any::Any, collections::BTreeMap, marker::PhantomData, sync::Mutex};

use crate::{
    abci::App,
    call::Call,
    encoding::{Decode, Encode},
    plugins::{ABCIPlugin, QueryPlugin},
    query::Query,
    state::State,
    store::{log::ReadLog, BackingStore, PartialMapStore, Read, Shared, Store, Write},
    Error, Result,
};

use super::exec::Transport;

/// A mock client for use in tests.
#[derive(Default)]
pub struct MockClient<T> {
    /// Encoded queries.
    pub queries: Mutex<Vec<Vec<u8>>>,
    /// Encoded calls.
    pub calls: Mutex<Vec<Vec<u8>>>,
    /// The client's store.
    pub store: Store,
    _marker: PhantomData<fn(T)>,
}

impl<T> MockClient<T> {
    /// Create a new mock client with the given store.
    pub fn with_store(store: Store) -> Self {
        Self {
            queries: Mutex::new(vec![]),
            calls: Mutex::new(vec![]),
            store,
            _marker: PhantomData,
        }
    }
}

impl<T: App + State + Query + Call> Transport<ABCIPlugin<QueryPlugin<T>>>
    for MockClient<ABCIPlugin<QueryPlugin<T>>>
{
    async fn query(&self, query: <ABCIPlugin<QueryPlugin<T>> as Query>::Query) -> Result<Store> {
        let query_bytes = query.encode()?;
        self.queries.lock().unwrap().push(query_bytes);

        let store = Store::new(BackingStore::Other(Shared::new(Box::new(ReadLog::new(
            self.store.clone(),
        )))));

        let root_bytes = store.get(&[])?.unwrap_or_default();
        let app = ABCIPlugin::<QueryPlugin<T>>::load(store.clone(), &mut root_bytes.as_slice())?;
        app.query(query)?;
        drop(app);

        let mut log = if let BackingStore::Other(b) = store.into_backing_store().into_inner() {
            let b = b.into_inner() as Box<dyn Any>;
            b.downcast::<ReadLog<Store>>().unwrap().reads().clone()
        } else {
            unreachable!()
        };

        // TODO: move to PartialMapStore associated function
        let mut out = BTreeMap::new();
        let mut insert = |key: Vec<u8>, value| {
            let prev = self.store.get_prev(Some(key.as_slice()))?;
            let contiguous = if let Some((pk, _)) = prev {
                out.contains_key(pk.as_slice())
            } else {
                true
            };

            out.remove(&key);
            out.insert(key, (contiguous, value));

            Ok::<_, Error>(())
        };
        let mut right_edge = false;
        log.sort();
        for key in log {
            match self.store.get(&key)? {
                Some(value) => insert(key, value)?,
                None => {
                    let prev = self.store.get_prev(Some(key.as_slice()))?;
                    if let Some((pk, pv)) = prev {
                        insert(pk, pv)?;
                    }

                    let next = self.store.get_next(&key)?;
                    if let Some((nk, nv)) = next {
                        insert(nk, nv)?;
                    } else {
                        right_edge = true;
                    }
                }
            }
        }
        let out_store = PartialMapStore::from_map(out, right_edge);

        Ok(Store::new(BackingStore::PartialMapStore(Shared::new(
            out_store,
        ))))
    }

    async fn call(&self, call: <ABCIPlugin<QueryPlugin<T>> as Call>::Call) -> Result<()> {
        self.calls.lock().unwrap().push(call.encode()?);

        let root_bytes = self.store.get(&[])?.unwrap_or_default();
        let mut app =
            ABCIPlugin::<QueryPlugin<T>>::load(self.store.clone(), &mut root_bytes.as_slice())?;
        let call = <ABCIPlugin<QueryPlugin<T>> as Call>::Call::decode(call.encode()?.as_slice())?;
        app.call(call)?;

        let mut out = vec![];
        app.flush(&mut out)?;
        self.store.clone().put(vec![], out)?;

        Ok(())
    }
}
