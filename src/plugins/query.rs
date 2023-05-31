use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::orga;
use crate::query::Query as QueryTrait;
use crate::store::{Read, Store};
use crate::Result;
use educe::Educe;
use std::cell::RefCell;

#[orga(skip(Query, Call))]
// TODO: #[call(transparent)]
pub struct QueryPlugin<T> {
    store: Store,
    #[state(transparent)]
    pub inner: RefCell<T>,
}

impl<T: Call> Call for QueryPlugin<T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.inner.borrow_mut().call(call)
    }
}

#[derive(Clone, Encode, Decode, Educe)]
#[educe(Debug)]
pub enum Query<T: QueryTrait + Call> {
    Query(T::Query),
    Call(T::Call),
    RawKey(Vec<u8>),
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
            Query::RawKey(key) => self.store.get(&key).map(|_| ()),
        }
    }
}

// TODO: Remove dependency on ABCI for this otherwise-pure plugin.
#[cfg(feature = "abci")]
mod abci {
    use super::super::{BeginBlockCtx, EndBlockCtx, InitChainCtx};
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};
    use crate::state::State;

    impl<T> BeginBlock for QueryPlugin<T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T> EndBlock for QueryPlugin<T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T> InitChain for QueryPlugin<T>
    where
        T: InitChain + State + Call,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.borrow_mut().init_chain(ctx)
        }
    }

    impl<T> crate::abci::AbciQuery for QueryPlugin<T>
    where
        T: crate::abci::AbciQuery + State + Call,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::abci::RequestQuery,
        ) -> Result<tendermint_proto::abci::ResponseQuery> {
            self.inner.abci_query(request)
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
