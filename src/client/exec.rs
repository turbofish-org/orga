use std::any::TypeId;

use super::{trace::take_trace, Client};
use crate::{
    describe::{Children, Describe, Descriptor, KeyOp},
    encoding::{Decode, Encode},
    merk::{BackingStore, ProofStore},
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

pub async fn execute<T, U>(
    store: Store,
    client: &impl Client,
    query_fn: impl Fn(T) -> Result<U>,
) -> Result<U>
where
    T: State + Query + Describe,
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
        let res = match step(store.clone(), &query_fn)? {
            StepResult::Done(value) => return Ok(value),
            StepResult::FetchKey(key, n) => {
                check_n(n)?;
                let query = QueryPluginQuery::Key(key).encode()?;
                client.query(query.as_slice()).await?
            }
            StepResult::FetchQuery(query, n) => {
                check_n(n)?;
                let query = QueryPluginQuery::Query(query.encode()?).encode()?;
                client.query(query.as_slice()).await?
            }
        };

        store = join_store(store, res)?;
    }
}

#[derive(Encode, Decode)]
enum QueryPluginQuery {
    Key(Vec<u8>),
    Query(Vec<u8>),
}

pub fn step<T, U>(store: Store, query_fn: impl Fn(T) -> Result<U>) -> Result<StepResult<T, U>>
where
    T: State + Query + Describe,
{
    let root_bytes = store.get(&[])?.unwrap_or_default();
    let app = T::load(store, &mut root_bytes.as_slice())?;

    let key = match query_fn(app) {
        Ok(value) => return Ok(StepResult::Done(value)),
        Err(Error::StoreErr(store::Error::ReadUnknown(key))) => key,
        Err(other_err) => return Err(other_err),
    };

    let (traces, push_count) = take_trace();
    if let Some(trace) = traces.first() {
        let res = T::describe().resolve_by_type_id(trace.type_id, key.as_slice(), vec![], vec![]);
        let receiver_pfx = match res {
            Ok(pfx) => pfx,
            Err(_) => return Ok(StepResult::FetchKey(key, push_count)),
        };
        let query_bytes = [receiver_pfx, trace.bytes()].concat();
        dbg!(&query_bytes);
        let query = T::Query::decode(query_bytes.as_slice())?;
        dbg!(&query);
        return Ok(StepResult::FetchQuery(query, push_count));
    }

    Ok(StepResult::FetchKey(key, 0))
}

pub fn join_store(dst: Store, src: Store) -> Result<Store> {
    let dst = dst.into_backing_store().into_inner();
    let src = src.into_backing_store().into_inner();

    match (dst, src) {
        (dst, BackingStore::Null(_)) => Ok(Store::new(dst)),
        (BackingStore::PartialMapStore(dst), BackingStore::PartialMapStore(src)) => {
            let mut dst = dst.into_inner();
            for (k, v) in src.into_inner().into_map() {
                assert_eq!(&v, &dst.get(&k)?);
                dst.put(k, v.unwrap())?;
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

    #[orga]
    struct Foo {
        pub bar: u32,
        pub baz: Deque<Deque<u32>>,
    }

    fn setup() -> (Store, MockClient<Foo>) {
        let mut store = Store::with_partial_map_store();

        let mut foo = Foo::default();
        foo.attach(store.clone()).unwrap();

        foo.bar = 123;

        let mut d = Deque::new();
        d.push_back(1).unwrap();
        d.push_back(2).unwrap();
        d.push_back(3).unwrap();
        foo.baz.push_back(d).unwrap();

        let mut d = Deque::new();
        foo.baz.push_back(d).unwrap();

        let mut bytes = vec![];
        foo.flush(&mut bytes).unwrap();
        store.put(vec![], bytes).unwrap();

        (store, MockClient::default())
    }

    #[tokio::test]
    async fn execute_simple() {
        let (store, client) = setup();

        let res = execute(store, &client, |app: Foo| Ok(app.bar))
            .await
            .unwrap();
        assert_eq!(res, 123);
        assert!(client.queries.borrow().is_empty());
    }

    #[tokio::test]
    async fn execute_deque_access() {
        let (store, client) = setup();

        let res = execute(store, &client, |app: Foo| Ok(app.baz.get(123)?.is_some()))
            .await
            .unwrap();
        assert!(res);
        // assert!(client.calls.borrow().is_empty());
        // assert!(client.queries.borrow().is_empty());
    }
}
