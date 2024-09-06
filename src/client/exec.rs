//! Client execution logic
use std::{any::TypeId, collections::HashSet};

use super::trace::{take_trace, tracing_guard};
use crate::{
    abci::App,
    call::Call,
    describe::{Children, Describe, Descriptor, KeyOp},
    encoding::{Decode, Encode},
    plugins::{query::QueryPlugin, ABCIPlugin},
    query::Query,
    state::State,
    store::{self, BackingStore, Read, Shared, Store},
    Error, Result,
};

#[cfg(feature = "merk-verify")]
use crate::merk::ProofStore;

/// Result of a single execution step of the client.
#[derive(Debug, Clone)]
pub enum StepResult<T: Query, U> {
    /// The query has been executed and the result is ready.
    Done(U),
    /// A required key was not and should be fetched.
    FetchKey(Vec<u8>),
    /// The next key for the provided key was not known and should be fetched.
    FetchNext(Vec<u8>),
    /// The previous key for the provided key was not known and should be
    /// fetched.
    FetchPrev(Option<Vec<u8>>),
    /// A query should be transmitted, and the resulting proof integrated into
    /// the client's store.
    FetchQuery(T::Query),
}

/// Trait for asynchronously executing queries and calls.
#[allow(async_fn_in_trait)]
pub trait Transport<T: Query + Call>: Send + Sync {
    /// Fetch the result for a and return a store containing the newly-fetched
    /// entries. The returned store will be joined into the client's local
    /// store.
    fn query(&self, query: T::Query) -> impl std::future::Future<Output = Result<Store>> + Send;

    /// Transmit a call.
    fn call(&self, call: T::Call) -> impl std::future::Future<Output = Result<()>> + Send;
}

impl<T: Transport<U>, U: Query + Call> Transport<U> for &mut T {
    async fn query(&self, query: <U as Query>::Query) -> Result<Store> {
        (**self).query(query).await
    }

    async fn call(&self, call: <U as Call>::Call) -> Result<()> {
        (**self).call(call).await
    }
}

// TODO: remove need for ABCIPlugin wrapping at this level, and App bound
/// Perform a client operation.
///
/// The provided `query_fn` will be called repeatedly:
///
/// Each time `query_fn` returns a [store::Error], the missing data will be
/// fetched and joined into the local store, then we call the function again.
///
/// If `query_fn` returns any other `Result`, we return that result directly.
///
/// The function will return when `query_fn` returns [StepResult::Done].
///
/// If the client errors because it's missing store data which we've already
/// attempted to fetch, we return an error.
pub async fn execute<T, U>(
    store: Store,
    client: &impl Transport<ABCIPlugin<QueryPlugin<T>>>,
    mut query_fn: impl FnMut(ABCIPlugin<QueryPlugin<T>>) -> Result<U>,
) -> Result<(U, Store)>
where
    T: App + State + Query + Call + Describe,
    T::Query: Send + Sync,
    T::Call: Send + Sync,
{
    let mut store = store;

    let mut queries = HashSet::new();

    loop {
        let query = match step(store.clone(), &mut query_fn)? {
            StepResult::Done(value) => return Ok((value, store)),
            StepResult::FetchKey(key) => QueryPluginQuery::RawKey(key),
            StepResult::FetchNext(key) => QueryPluginQuery::RawNext(key),
            StepResult::FetchPrev(key) => QueryPluginQuery::RawPrev(key),
            StepResult::FetchQuery(query) => QueryPluginQuery::Query(query),
        };

        let query_bytes = query.encode()?;
        if queries.contains(&query_bytes) {
            return Err(Error::Client("Execution did not advance".into()));
        }
        queries.insert(query_bytes);

        let res = client.query(query).await?;

        store = join_store(store, res)?;
    }
}

type QueryPluginQuery<T> = <QueryPlugin<T> as Query>::Query;

/// Perform a single step of the client execution.
///
/// We execute the provided `query_fn` with an initial store which may or may
/// not contain the data needed to execute the operation. See [execute] for
/// details on how this lifecycle is managed.
///
/// In this function, we use a thread-local tracing system to track which
/// `#[query]` method we've called most recently and its arguments, and use the
/// type's [Descriptor] to determine the full bytes of the encoded query that
/// should be transported to the remote node to determine which data should be
/// proven to us.
pub fn step<T, U>(
    store: Store,
    mut query_fn: impl FnMut(ABCIPlugin<QueryPlugin<T>>) -> Result<U>,
) -> Result<StepResult<T, U>>
where
    T: App + State + Query + Describe,
{
    let _guard = tracing_guard();
    take_trace();

    let root_bytes = match store.get(&[]) {
        Err(Error::StoreErr(store::Error::GetUnknown(_))) | Ok(None) => {
            return Ok(StepResult::FetchKey(vec![]))
        }
        Err(err) => return Err(err),
        Ok(Some(bytes)) => bytes,
    };

    let app = ABCIPlugin::<QueryPlugin<T>>::load(store, &mut &root_bytes[..])?;

    let (key, fallback_res) = match query_fn(app) {
        Err(Error::StoreErr(store::Error::GetUnknown(key))) => {
            (key.clone(), StepResult::FetchKey(key))
        }
        Err(Error::StoreErr(store::Error::GetNextUnknown(key))) => {
            (key.clone(), StepResult::FetchNext(key))
        }
        Err(Error::StoreErr(store::Error::GetPrevUnknown(maybe_key))) => {
            if let Some(key) = maybe_key {
                (key.clone(), StepResult::FetchPrev(Some(key)))
            } else {
                return Err(Error::StoreErr(store::Error::GetPrevUnknown(None)));
            }
        }
        Err(other_err) => return Err(other_err),
        Ok(value) => return Ok(StepResult::Done(value)),
    };

    let traces = take_trace();
    if let Some(trace) = traces.history.last() {
        let res = ABCIPlugin::<QueryPlugin<T>>::describe().resolve_by_type_id(
            trace.type_id,
            key.as_slice(),
            vec![],
            vec![],
        );
        let receiver_pfx = match res {
            Ok(pfx) => pfx,
            Err(_) => return Ok(fallback_res),
        };
        let query_bytes = [
            // TODO: shouldn't have to cut off ABCIPlugin prefixes here
            receiver_pfx[1..].to_vec(),
            trace.bytes(),
        ]
        .concat();
        let query = T::Query::decode(query_bytes.as_slice())?;
        return Ok(StepResult::FetchQuery(query));
    }

    Ok(fallback_res)
}

/// Join two partial stores together.
pub fn join_store(dst: Store, src: Store) -> Result<Store> {
    let dst = dst.into_backing_store().into_inner();
    let src = src.into_backing_store().into_inner();

    match (dst, src) {
        (store, BackingStore::Null(_)) | (BackingStore::Null(_), store) => Ok(Store::new(store)),
        (BackingStore::PartialMapStore(dst), BackingStore::PartialMapStore(src)) => {
            let dst = dst.into_inner();
            let src = src.into_inner();
            let joined = dst.join(src);
            Ok(Store::new(BackingStore::PartialMapStore(Shared::new(
                joined,
            ))))
        }
        #[cfg(feature = "merk-verify")]
        (BackingStore::ProofMap(dst), BackingStore::ProofMap(src)) => {
            let dst = dst.into_inner();
            let src = src.into_inner();
            let joined = dst.0.join(src.0);
            Ok(Store::new(BackingStore::ProofMap(Shared::new(ProofStore(
                joined,
            )))))
        }
        _ => Err(Error::Client(
            "Could not join mismatched or unsupported store types".into(),
        )),
    }
}

impl Descriptor {
    /// Resolve the key prefix for a value in the state hierarchy based on data
    /// collected during traced execution.
    pub fn resolve_by_type_id(
        &self,
        target_type_id: TypeId,
        read_key: &[u8],
        mut self_store_key: Vec<u8>,
        mut out_bytes: Vec<u8>,
    ) -> Result<Vec<u8>> {
        if self.type_id == target_type_id {
            return Ok(out_bytes);
        }

        let child_key = &read_key[self_store_key.len()..];
        match self.children() {
            Children::None => Err(Error::Client("No matching child".to_string())),
            Children::Named(children) => {
                for child in children {
                    match child.store_key {
                        KeyOp::Append(ref bytes) => {
                            if child_key.starts_with(bytes) {
                                return child.desc.resolve_by_type_id(
                                    target_type_id,
                                    read_key,
                                    child.store_key.apply_bytes(self_store_key.as_slice()),
                                    child.store_key.apply_bytes(out_bytes.as_slice()),
                                );
                            }
                        }
                        _ => continue,
                    }
                }
                Err(Error::Client("No matching child".to_string()))
            }
            Children::Dynamic(child) => {
                let consumed = child.key_desc().encoding_bytes_subslice(child_key)?;
                out_bytes = child.apply_query_bytes(out_bytes);
                out_bytes.extend_from_slice(consumed);
                self_store_key.extend_from_slice(consumed);
                child.value_desc().resolve_by_type_id(
                    target_type_id,
                    read_key,
                    self_store_key,
                    out_bytes,
                )
            }
        }
    }

    /// Consume bytes from the provided slice according to this descriptor's
    /// load function.
    pub fn encoding_bytes_subslice<'a>(&self, bytes: &'a [u8]) -> Result<&'a [u8]> {
        let store = Store::default();
        let mut consume_bytes = bytes;
        if let Some(load) = self.load {
            load(store, &mut consume_bytes)?;
            Ok(&bytes[..bytes.len() - consume_bytes.len()])
        } else {
            Err(Error::Client("No load function".to_string()))
        }
    }
}

#[cfg(test)]
#[cfg(feature = "tokio")]
mod tests {
    use super::*;
    use crate::client::mock::MockClient;
    use crate::collections::Deque;
    use crate::orga;
    use crate::plugins::query::QueryPlugin;
    use crate::store::Write;

    #[orga]
    struct Foo {
        pub bar: u32,
        pub baz: Deque<Deque<u32>>,
    }

    #[orga]
    impl Foo {
        #[query]
        fn iter_query(&self) -> Result<u64> {
            Ok(self.baz.iter()?.collect::<Result<Vec<_>>>()?.len() as u64)
        }

        #[query]
        fn iter_query_rev(&self) -> Result<u64> {
            Ok(self.baz.iter()?.rev().collect::<Result<Vec<_>>>()?.len() as u64)
        }
    }

    fn setup() -> MockClient<ABCIPlugin<QueryPlugin<Foo>>> {
        let mut client = MockClient::default();
        client.store = Store::with_map_store();

        let mut foo = ABCIPlugin::<QueryPlugin<Foo>>::default();
        foo.attach(client.store.clone()).unwrap();

        foo.inner.inner.borrow_mut().bar = 123;

        let mut d = Deque::new();
        d.push_back(1).unwrap();
        d.push_back(2).unwrap();
        d.push_back(3).unwrap();
        foo.inner.inner.borrow_mut().baz.push_back(d).unwrap();

        let d = Deque::new();
        foo.inner.inner.borrow_mut().baz.push_back(d).unwrap();

        let mut d = Deque::new();
        d.push_back(10).unwrap();
        foo.inner.inner.borrow_mut().baz.push_back(d).unwrap();

        let mut bytes = vec![];
        foo.flush(&mut bytes).unwrap();
        client.store.put(vec![], bytes).unwrap();

        client
    }

    #[tokio::test]
    async fn execute_simple() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            Ok(app.inner.inner.borrow().bar)
        })
        .await
        .unwrap();
        assert_eq!(res, 123);
        assert_eq!(client.queries.into_inner().unwrap(), vec![vec![2]]);
    }

    #[tokio::test]
    async fn execute_deque_access_none() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            Ok(app.inner.inner.borrow().baz.get(123)?.is_none())
        })
        .await
        .unwrap();
        assert!(res);
        assert_eq!(
            client.queries.into_inner().unwrap(),
            vec![vec![2], vec![0, 1, 131, 0, 0, 0, 0, 0, 0, 0, 123]]
        );
    }

    #[tokio::test]
    async fn execute_deque_access_some() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            Ok(*app
                .inner
                .inner
                .borrow()
                .baz
                .get(0)?
                .unwrap()
                .get(2)?
                .unwrap())
        })
        .await
        .unwrap();

        assert_eq!(res, 3);
        assert_eq!(
            client.queries.into_inner().unwrap(),
            vec![
                vec![2],
                vec![0, 1, 131, 0, 0, 0, 0, 0, 0, 0, 0],
                vec![
                    0, 1, 129, 127, 255, 255, 255, 255, 255, 255, 255, 131, 0, 0, 0, 0, 0, 0, 0, 2
                ]
            ]
        );
    }

    #[tokio::test]
    async fn execute_iter_raw() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            Ok(app
                .inner
                .inner
                .borrow()
                .baz
                .iter()?
                .collect::<Result<Vec<_>>>()?
                .len())
        })
        .await
        .unwrap();

        assert_eq!(res, 3);
        assert_eq!(
            client.queries.into_inner().unwrap(),
            vec![
                vec![2],
                vec![3, 0, 1],
                vec![2, 0, 1, 128, 0, 0, 0, 0, 0, 0, 0],
                vec![2, 0, 1, 128, 0, 0, 0, 0, 0, 0, 1],
                vec![2, 0, 1, 128, 0, 0, 0, 0, 0, 0, 2],
            ]
        );
    }

    #[tokio::test]
    async fn execute_iter_query() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            app.inner.inner.borrow().iter_query()
        })
        .await
        .unwrap();

        assert_eq!(res, 3);
        assert_eq!(
            client.queries.into_inner().unwrap(),
            vec![vec![2], vec![0, 128]]
        );
    }

    #[tokio::test]
    async fn execute_iter_rev_raw() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            Ok(app
                .inner
                .inner
                .borrow()
                .baz
                .iter()?
                .rev()
                .collect::<Result<Vec<_>>>()?
                .len())
        })
        .await
        .unwrap();

        assert_eq!(res, 3);
        assert_eq!(
            client.queries.into_inner().unwrap(),
            vec![
                vec![2],
                vec![4, 1, 0, 2],
                vec![2, 0, 1, 128, 0, 0, 0, 0, 0, 0, 1],
                vec![2, 0, 1, 128, 0, 0, 0, 0, 0, 0, 0],
                vec![2, 0, 1, 127, 255, 255, 255, 255, 255, 255, 255]
            ]
        );
    }

    #[tokio::test]
    async fn execute_iter_rev_query() {
        let client = setup();

        let (res, _store) = execute(Store::default(), &client, |app| {
            app.inner.inner.borrow().iter_query_rev()
        })
        .await
        .unwrap();

        assert_eq!(res, 3);
        assert_eq!(
            client.queries.into_inner().unwrap(),
            vec![vec![2], vec![0, 129]]
        );
    }
}
