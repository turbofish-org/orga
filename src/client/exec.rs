use std::any::TypeId;

use super::{trace::take_trace, Client};
use crate::{
    describe::{Child, Children, Describe, Descriptor, InspectRef, KeyOp, WithFn},
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
                let query = query.encode()?;
                let res = client.query(query.as_slice()).await?;
                res
            }
        };

        store = join_store(store, res)?;
    }
}

#[derive(Encode, Decode)]
enum QueryPluginQuery {
    Query(Vec<u8>),
    Foo,
    Key(Vec<u8>),
}

pub fn step<T, U>(store: Store, query_fn: impl Fn(T) -> Result<U>) -> Result<StepResult<T, U>>
where
    T: State + Query + Describe,
{
    let root_bytes = match store.get(&[])? {
        Some(bytes) => bytes,
        None => return Ok(StepResult::FetchKey(vec![], 0)),
    };
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
    pub fn resolve_by_key(&self, subkey: &[u8]) -> Result<Vec<Child>> {
        if subkey.is_empty() {
            return Ok(vec![]);
        }
        let (consumed, child) = self
            .children()
            .child_by_key(subkey)
            .ok_or_else(|| Error::Client(format!("No child found for key {:?}", subkey)))?;
        let child_desc = child.describe();
        let resolved_children = child_desc.resolve_by_key(&subkey[consumed.len()..])?;
        let mut children = vec![child];
        children.extend(resolved_children);

        Ok(children)
    }

    pub fn access_by_key(&self, subkey: &[u8], instance: InspectRef, op: WithFn) {
        if subkey.is_empty() {
            return op(instance);
        }
        let (consumed, child) = self.children().child_by_key(subkey).unwrap();
        let child_desc = child.describe().clone();
        child.access(instance, &mut |v| {
            child_desc.access_by_key(&subkey[consumed.len()..], v, op);
        });
    }

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
    use crate::prelude::query::QueryPlugin;

    #[orga]
    struct Foo {
        pub bar: u32,
        pub baz: Deque<Deque<u32>>,
    }

    fn setup() -> MockClient<QueryPlugin<Foo>> {
        let mut client = MockClient::default();
        client.store = Store::with_map_store();

        let mut foo = QueryPlugin::<Foo>::default();
        foo.attach(client.store.clone()).unwrap();

        foo.inner.borrow_mut().bar = 123;

        let mut d = Deque::new();
        d.push_back(1).unwrap();
        d.push_back(2).unwrap();
        d.push_back(3).unwrap();
        foo.inner.borrow_mut().baz.push_back(d).unwrap();

        let d = Deque::new();
        foo.inner.borrow_mut().baz.push_back(d).unwrap();

        let mut bytes = vec![];
        foo.flush(&mut bytes).unwrap();
        client.store.put(vec![], bytes).unwrap();

        client
    }

    #[tokio::test]
    async fn execute_simple() {
        let client = setup();

        let res = execute(Store::default(), &client, |app: QueryPlugin<Foo>| {
            Ok(app.inner.borrow().bar)
        })
        .await
        .unwrap();
        assert_eq!(res, 123);
        assert_eq!(client.queries.take(), vec![vec![2]]);
    }

    #[tokio::test]
    async fn execute_deque_access_none() {
        let client = setup();

        let res = execute(Store::default(), &client, |app: QueryPlugin<Foo>| {
            Ok(app.inner.borrow().baz.get(123)?.is_none())
        })
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

        let res = execute(Store::default(), &client, |app: QueryPlugin<Foo>| {
            Ok(*app.inner.borrow().baz.get(0)?.unwrap().get(2)?.unwrap())
        })
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
