use super::sdk_compat::{sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call;
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::{Coin, Symbol};
use crate::context::GetContext;
use crate::plugins::Paid;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub const MIN_FEE: u64 = 10_000;

pub struct FeePlugin<S, T> {
    inner: T,
    _symbol: PhantomData<S>,
}

impl<S, T> State for FeePlugin<S, T>
where
    S: Symbol,
    T: State,
{
    type Encoding = (T::Encoding,);
    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store, data.0)?,
            _symbol: PhantomData,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<S, T> From<FeePlugin<S, T>> for (T::Encoding,)
where
    T: State,
{
    fn from(provider: FeePlugin<S, T>) -> Self {
        (provider.inner.into(),)
    }
}

impl<S, T: Query> Query for FeePlugin<S, T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<S: Symbol, T: Call + State> Call for FeePlugin<S, T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        // let paid = self
        //     .context::<Paid>()
        //     .ok_or_else(|| Error::Coins("Minimum fee not paid".into()))?;

        // if !paid.running_payer {
        //     let fee_payment: Coin<S> = paid.take(MIN_FEE)?;
        //     fee_payment.burn();
        // }

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
}
