use serde::{Deserialize, Serialize};

use super::sdk_compat::{sdk::Tx as SdkTx, ConvertSdkTx};
use super::Paid;
use crate::call::Call;
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::{Coin, Symbol};
use crate::context::GetContext;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::MigrateFrom;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub const MIN_FEE: u64 = 10_000;

#[derive(Encode, Decode, Default, Serialize, Deserialize, MigrateFrom)]
#[serde(transparent)]
pub struct FeePlugin<S, T> {
    #[serde(skip)]
    _symbol: PhantomData<S>,
    inner: T,
}

impl<S, T> State for FeePlugin<S, T>
where
    S: Symbol,
    T: State,
{
    fn attach(&mut self, store: Store) -> Result<()> {
        self.inner.attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        self.inner.flush(out)
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let inner = T::load(store, bytes)?;
        Ok(Self {
            _symbol: PhantomData,
            inner,
        })
    }
}

// impl<S, T> Describe for FeePlugin<S, T>
// where
//     S: Symbol,
//     T: State + Describe + 'static,
// {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>()
//             .named_child::<T>("inner", &[], |v| {
//                 crate::describe::Builder::access(v, |v: Self| v.inner)
//             })
//             .build()
//     }
// }

impl<S, T: Query> Query for FeePlugin<S, T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<S: Symbol, T: Call + State> Call for FeePlugin<S, T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("Minimum fee not paid".into()))?;

        if !paid.running_payer {
            let fee_payment: Coin<S> = paid.take(MIN_FEE)?;
            fee_payment.burn();
        }

        self.inner.call(call)
    }
}

impl<S, T: ConvertSdkTx> ConvertSdkTx for FeePlugin<S, T> {
    type Output = T::Output;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<T::Output> {
        self.inner.convert(sdk_tx)
    }
}

pub struct FeeAdapter<T, U: Clone, S> {
    parent: U,
    marker: std::marker::PhantomData<fn(S, T)>,
}

unsafe impl<T, U: Send + Clone, S> Send for FeeAdapter<T, U, S> {}

impl<T, U: Clone, S> Clone for FeeAdapter<T, U, S> {
    fn clone(&self) -> Self {
        FeeAdapter {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Call, U: AsyncCall<Call = T::Call> + Clone, S> AsyncCall for FeeAdapter<T, U, S>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        self.parent.call(call).await
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query + State,
        U: for<'a> AsyncQuery<Query = T::Query, Response<'a> = std::rc::Rc<FeePlugin<S, T>>> + Clone,
        S,
    > AsyncQuery for FeeAdapter<T, U, S>
{
    type Query = T::Query;
    type Response<'a> = std::rc::Rc<T>;

    async fn query<F, R>(&self, query: Self::Query, mut check: F) -> Result<R>
    where
        F: FnMut(Self::Response<'_>) -> Result<R>,
    {
        self.parent
            .query(query, |plugin| {
                check(std::rc::Rc::new(
                    std::rc::Rc::try_unwrap(plugin)
                        .map_err(|_| ())
                        .unwrap()
                        .inner,
                ))
            })
            .await
    }
}

impl<S, T: Client<FeeAdapter<T, U, S>>, U: Clone> Client<U> for FeePlugin<S, T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(FeeAdapter {
            parent,
            marker: std::marker::PhantomData,
        })
    }
}

impl<S, T> Deref for FeePlugin<S, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<S, T> DerefMut for FeePlugin<S, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

// TODO: Remove dependency on ABCI for this otherwise-pure plugin.
#[cfg(feature = "abci")]
mod abci {
    use super::super::{BeginBlockCtx, EndBlockCtx, InitChainCtx};
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<S, T> BeginBlock for FeePlugin<S, T>
    where
        S: Symbol,
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<S, T> EndBlock for FeePlugin<S, T>
    where
        S: Symbol,
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<S, T> InitChain for FeePlugin<S, T>
    where
        S: Symbol,
        T: InitChain + State + Call,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }

    impl<S, T> crate::abci::AbciQuery for FeePlugin<S, T>
    where
        S: Symbol,
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
