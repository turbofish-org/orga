//! Low-level query operations.
use crate::call::Call;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::Migrate;
use crate::orga;
use crate::query::Query as QueryTrait;
use crate::state::State;
use crate::store::{Read, Store};
use crate::Result;
use educe::Educe;
use serde::Serialize;
use std::cell::RefCell;

/// A plugin which adds low-level query operations to its query implementation,
/// such as raw store reads and call simulation.
#[derive(Default, Serialize)]
pub struct QueryPlugin<T> {
    store: Store,
    /// A [RefCell] of the inner type to support interior mutability, e.g. in
    /// call simulation.
    pub inner: RefCell<T>,
}

impl<T: State> State for QueryPlugin<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.store = store.clone();
        self.inner.attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        self.inner.flush(out)
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let inner = T::load(store.clone(), bytes)?;
        Ok(Self {
            store,
            inner: RefCell::new(inner),
        })
    }

    fn field_keyop(field_name: &str) -> Option<orga::describe::KeyOp> {
        match field_name {
            "store" => Some(orga::describe::KeyOp::Append(vec![])),
            "inner" => Some(orga::describe::KeyOp::Append(vec![])),
            _ => None,
        }
    }
}

impl<T: Call> Call for QueryPlugin<T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.inner.call(call)
    }
}

/// The query type for [QueryPlugin].
#[derive(Clone, Encode, Decode, Educe)]
#[educe(Debug)]
pub enum Query<T: QueryTrait + Call> {
    /// Pass the query along to the inner value.
    Query(T::Query),
    /// Simulate a call.
    Call(T::Call),
    /// Read a raw key from the store.
    RawKey(Vec<u8>),
    /// Get the next key in the store from the provided key.
    RawNext(Vec<u8>),
    /// Get the previous key in the store from the provided key.
    RawPrev(Option<Vec<u8>>),
}

impl<T> QueryTrait for QueryPlugin<T>
where
    T: QueryTrait + Call,
{
    type Query = Query<T>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Query::Query(query) => self.inner.borrow().query(query),
            Query::Call(call) => self.inner.borrow_mut().call(call),
            Query::RawKey(key) => self.store.with_prefix(vec![]).get(&key).map(|_| ()),
            Query::RawNext(key) => self.store.with_prefix(vec![]).get_next(&key).map(|_| ()),
            Query::RawPrev(key) => self
                .store
                .with_prefix(vec![])
                .get_prev(key.as_deref())
                .map(|_| ()),
        }
    }
}

impl<T: Migrate> Migrate for QueryPlugin<T> {
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            store: dest.clone(),
            inner: RefCell::new(T::migrate(src, dest, bytes)?),
        })
    }
}

impl<T: State + Describe> Describe for QueryPlugin<T> {
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>()
            .meta::<u8>()
            .named_child::<RefCell<T>>("inner", &[])
            .build()
    }
}

// TODO: Remove dependency on ABCI for this otherwise-pure plugin.
#[cfg(feature = "abci")]
mod abci {
    use std::ops::{Deref, DerefMut};

    use super::super::{BeginBlockCtx, EndBlockCtx, InitChainCtx};
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};
    use crate::state::State;

    impl<T> BeginBlock for QueryPlugin<T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.borrow_mut().deref_mut().begin_block(ctx)
        }
    }

    impl<T> EndBlock for QueryPlugin<T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.borrow_mut().deref_mut().end_block(ctx)
        }
    }

    impl<T> InitChain for QueryPlugin<T>
    where
        T: InitChain + State + Call,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.borrow_mut().deref_mut().init_chain(ctx)
        }
    }

    impl<T> crate::abci::AbciQuery for QueryPlugin<T>
    where
        T: crate::abci::AbciQuery + State + Call,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::v0_34::abci::RequestQuery,
        ) -> Result<tendermint_proto::v0_34::abci::ResponseQuery> {
            self.inner.borrow().deref().abci_query(request)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::build_call;
    use crate::call::FieldCall;
    use crate::query::FieldQuery;
    use crate::state::State;

    #[derive(State, FieldCall, Default, Debug)]
    struct Bloop {
        #[call]
        pub app: Intermediate,
    }

    #[derive(State, FieldCall, Default, Debug)]
    pub struct Intermediate {
        #[call]
        pub baz: Baz,
        #[call]
        pub foo: Foo,
    }

    #[orga]
    #[derive(Debug)]
    pub struct Foo {
        pub a: u32,
        pub b: u32,
        pub c: (u32, u32),
        pub d: Baz,
    }

    #[orga]
    impl Foo {
        #[call]
        pub fn inc_a(&mut self, n: u32) -> Result<()> {
            self.a += n;

            Ok(())
        }
    }

    #[orga]
    #[derive(Debug)]
    pub struct MyApp {
        #[call]
        pub foo: Foo,
    }

    #[orga]
    #[derive(Debug)]
    pub struct Baz {
        beep: u32,
        boop: u8,
    }

    #[orga]
    impl Baz {
        #[call]
        pub fn inc_beep(&mut self, n: u32) -> Result<()> {
            self.beep += n;

            Ok(())
        }

        #[call]
        pub fn other_baz_method(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[derive(State, FieldCall, Default, FieldQuery)]
    pub struct Bleep {
        pub a: u32,
        pub b: u64,
    }

    #[orga]
    impl Bleep {
        #[query]
        fn my_query(&self, n: u32) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn call_sim() -> Result<()> {
        let mut bloop = Bloop::default();
        let client = &mut bloop;
        let call_one = build_call!(client.app.baz.inc_beep(10));
        let client = &mut bloop;
        let call_two = build_call!(client.app.baz.inc_beep(15));

        dbg!(&bloop);
        bloop.call(call_one)?;
        bloop.call(call_two)?;
        dbg!(&bloop);
        assert_eq!(bloop.app.baz.beep, 25);
        Ok(())
    }
}
