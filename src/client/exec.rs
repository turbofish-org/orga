use std::any::TypeId;

use super::trace::take_trace;
use crate::{
    abci::App,
    call::Call,
    describe::{Children, Describe, Descriptor, KeyOp},
    encoding::Decode,
    merk::{BackingStore, ProofStore},
    plugins::{query::QueryPlugin, ABCIPlugin},
    query::Query,
    state::State,
    store::{self, Read, Shared, Store, Write},
    Error, Result,
};

#[derive(Debug, Clone)]
pub enum StepResult<T: Query, U> {
    Done(U),
    FetchKey(Vec<u8>, u64),
    FetchQuery(T::Query, u64),
}

pub trait Client<T: Query + Call> {
    async fn query(&self, query: T::Query) -> Result<Store>;

    async fn call(&self, call: T::Call) -> Result<()>;
}

impl<T: Client<U>, U: Query + Call> Client<U> for &mut T {
    async fn query(&self, query: <U as Query>::Query) -> Result<Store> {
        (**self).query(query).await
    }

    async fn call(&self, call: <U as Call>::Call) -> Result<()> {
        (**self).call(call).await
    }
}

// TODO: remove need for ABCIPlugin wrapping at this level, and App bound

pub async fn execute<T, U>(
    store: Store,
    client: &impl Client<ABCIPlugin<QueryPlugin<T>>>,
    mut query_fn: impl FnMut(ABCIPlugin<QueryPlugin<T>>) -> Result<U>,
) -> Result<(U, Store)>
where
    T: App + State + Query + Call + Describe,
{
    let mut store = store;
    let mut last_n = None;

    let mut check_n = |n| {
        if let Some(last_n) = last_n {
            if n <= last_n {
                return Err(Error::Client("Execution did not advance".into()));
            }
        }

        last_n = Some(n);

        Ok(())
    };

    loop {
        let res = match step(store.clone(), &mut query_fn)? {
            StepResult::Done(value) => return Ok((value, store)),
            StepResult::FetchKey(key, n) => {
                check_n(n)?;
                client.query(QueryPluginQuery::RawKey(key)).await?
            }
            StepResult::FetchQuery(query, n) => {
                check_n(n)?;
                client.query(QueryPluginQuery::Query(query)).await?
            }
        };

        store = join_store(store, res)?;
    }
}

type QueryPluginQuery<T> = <QueryPlugin<T> as Query>::Query;

pub fn step<T, U>(
    store: Store,
    mut query_fn: impl FnMut(ABCIPlugin<QueryPlugin<T>>) -> Result<U>,
) -> Result<StepResult<T, U>>
where
    T: App + State + Query + Describe,
{
    take_trace();

    let root_bytes = match store.get(&[]) {
        Err(Error::StoreErr(store::Error::ReadUnknown(_))) | Ok(None) => {
            return Ok(StepResult::FetchKey(vec![], 0))
        }
        Err(err) => return Err(err),
        Ok(Some(bytes)) => bytes,
    };

    let app = ABCIPlugin::<QueryPlugin<T>>::load(store, &mut &root_bytes[..])?;

    let key = match query_fn(app) {
        Err(Error::StoreErr(store::Error::ReadUnknown(key))) => key,
        Err(other_err) => return Err(other_err),
        Ok(value) => return Ok(StepResult::Done(value)),
    };

    let (traces, push_count) = take_trace();
    if let Some(trace) = traces.first() {
        let res = ABCIPlugin::<QueryPlugin<T>>::describe().resolve_by_type_id(
            trace.type_id,
            key.as_slice(),
            vec![],
            vec![],
        );
        let receiver_pfx = match res {
            Ok(pfx) => pfx,
            Err(_) => return Ok(StepResult::FetchKey(key, push_count)),
        };
        let query_bytes = [
            // TODO: shouldn't have to cut off ABCIPlugin, QueryPlugin prefixes here
            receiver_pfx[2..].to_vec(),
            trace.bytes(),
        ]
        .concat();
        let query = T::Query::decode(query_bytes.as_slice())?;
        return Ok(StepResult::FetchQuery(query, push_count));
    }

    Ok(StepResult::FetchKey(key, 0))
}

pub fn join_store(dst: Store, src: Store) -> Result<Store> {
    let dst = dst.into_backing_store().into_inner();
    let src = src.into_backing_store().into_inner();

    match (dst, src) {
        (store, BackingStore::Null(_)) | (BackingStore::Null(_), store) => Ok(Store::new(store)),
        (BackingStore::PartialMapStore(dst), BackingStore::PartialMapStore(src)) => {
            let mut dst = dst.into_inner();
            for (k, v) in src.into_inner().into_map() {
                match v {
                    Some(v) => dst.put(k, v)?,
                    None => dst.delete(&k)?,
                }
            }
            Ok(Store::new(BackingStore::PartialMapStore(Shared::new(dst))))
        }
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
mod tests {
    use super::*;
    use crate::client::mock::MockClient;
    use crate::collections::Deque;
    use crate::orga;
    use crate::plugins::query::QueryPlugin;

    #[orga]
    struct Foo {
        pub bar: u32,
        pub baz: Deque<Deque<u32>>,
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

        let mut bytes = vec![];
        foo.flush(&mut bytes).unwrap();
        client.store.put(vec![], bytes).unwrap();

        client
    }

    #[tokio::test]
    async fn execute_simple() {
        let client = setup();

        let (res, _store) = execute(
            Store::default(),
            &client,
            |app: ABCIPlugin<QueryPlugin<Foo>>| Ok(app.inner.inner.borrow().bar),
        )
        .await
        .unwrap();
        assert_eq!(res, 123);
        assert_eq!(client.queries.take(), vec![vec![2]]);
    }

    #[tokio::test]
    async fn execute_deque_access_none() {
        let client = setup();

        let (res, _store) = execute(
            Store::default(),
            &client,
            |app: ABCIPlugin<QueryPlugin<Foo>>| {
                Ok(app.inner.inner.borrow().baz.get(123)?.is_none())
            },
        )
        .await
        .unwrap();
        assert!(res);
        assert_eq!(
            client.queries.take(),
            vec![vec![2], vec![0, 1, 131, 0, 0, 0, 0, 0, 0, 0, 123]]
        );
    }

    #[tokio::test]
    async fn execute_deque_access_some() {
        let client = setup();

        let (res, _store) = execute(
            Store::default(),
            &client,
            |app: ABCIPlugin<QueryPlugin<Foo>>| {
                Ok(*app
                    .inner
                    .inner
                    .borrow()
                    .baz
                    .get(0)?
                    .unwrap()
                    .get(2)?
                    .unwrap())
            },
        )
        .await
        .unwrap();

        assert_eq!(res, 3);
        assert_eq!(
            client.queries.take(),
            vec![
                vec![2],
                vec![0, 1, 131, 0, 0, 0, 0, 0, 0, 0, 0],
                vec![
                    0, 1, 129, 127, 255, 255, 255, 255, 255, 255, 255, 131, 0, 0, 0, 0, 0, 0, 0, 2
                ]
            ]
        );
    }
}
