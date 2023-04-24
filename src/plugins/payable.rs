use super::sdk_compat::{sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call;
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::{Amount, Coin, Symbol};
use crate::context::{Context, GetContext};
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::{MigrateFrom, MigrateInto};
use crate::query::{FieldQuery, Query};
use crate::state::State;
use crate::{Error, Result};
use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::{Deref, DerefMut};

const MAX_SUBCALL_LEN: u32 = 200_000;

#[derive(State, Default, FieldQuery, Describe)]
pub struct PayablePlugin<T> {
    pub inner: T,
}

impl<T1, T2> MigrateFrom<PayablePlugin<T1>> for PayablePlugin<T2>
where
    T1: MigrateInto<T2>,
{
    fn migrate_from(other: PayablePlugin<T1>) -> Result<Self> {
        Ok(Self {
            inner: other.inner.migrate_into()?,
        })
    }
}

#[derive(Default)]
pub struct Paid {
    map: HashMap<u8, Amount>,
    pub running_payer: bool,
    pub fee_disabled: bool,
}

impl Paid {
    pub fn give<S: Symbol, A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        self.give_denom(amount, S::INDEX)
    }

    pub fn give_denom<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        let entry = self.map.entry(denom).or_insert_with(|| 0.into());
        let amount = amount.into();
        *entry = (*entry + amount)?;

        Ok(())
    }

    pub fn take<S: Symbol, A: Into<Amount>>(&mut self, amount: A) -> Result<Coin<S>> {
        let amount = amount.into();
        self.take_denom(amount, S::INDEX)?;

        Ok(S::mint(amount))
    }

    pub fn take_denom<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        let entry = self.map.entry(denom).or_insert_with(|| 0.into());
        let amount = amount.into();
        if *entry < amount {
            return Err(Error::Coins("Insufficient funding for paid call".into()));
        }

        *entry = (*entry - amount)?;

        Ok(())
    }

    pub fn balance<S: Symbol>(&self) -> Result<Amount> {
        let entry = match self.map.get(&S::INDEX) {
            Some(amt) => *amt,
            None => 0.into(),
        };

        Ok(entry)
    }
}

#[derive(Debug)]
pub struct PaidCall<T> {
    pub payer: T,
    pub paid: T,
}

impl<T: Encode + std::fmt::Debug> Encode for PaidCall<T> {
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.payer.encoding_length()? + self.paid.encoding_length()? + 8)
    }
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        dbg!(&self.payer, &self.paid);
        let payer_call_bytes = self.payer.encode()?;
        dbg!(payer_call_bytes.len());
        let payer_call_len: u32 = payer_call_bytes
            .len()
            .try_into()
            .map_err(|_| ed::Error::UnexpectedByte(0))?;
        dbg!(payer_call_len, payer_call_len.encode()?);
        let paid_call_bytes = self.paid.encode()?;
        let paid_call_len: u32 = paid_call_bytes
            .len()
            .try_into()
            .map_err(|_| ed::Error::UnexpectedByte(0))?;
        dbg!(paid_call_len, paid_call_len.encode()?);

        dest.write_all(&payer_call_len.encode()?)?;
        dest.write_all(&payer_call_bytes)?;
        dest.write_all(&paid_call_len.encode()?)?;
        dest.write_all(&paid_call_bytes)?;

        Ok(())
    }
}

impl<T: Decode + std::fmt::Debug> Decode for PaidCall<T> {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let payer_call_len = u32::decode(&mut reader)?;
        if payer_call_len > MAX_SUBCALL_LEN {
            return Err(ed::Error::UnexpectedByte(32));
        }
        let mut payer_call_bytes = vec![0u8; payer_call_len as usize];
        reader.read_exact(&mut payer_call_bytes)?;
        let paid_call_len = u32::decode(&mut reader)?;
        if payer_call_len > MAX_SUBCALL_LEN {
            return Err(ed::Error::UnexpectedByte(32));
        }
        let mut paid_call_bytes = vec![0u8; paid_call_len as usize];
        reader.read_exact(&mut paid_call_bytes)?;
        Ok(Self {
            payer: dbg!(T::decode(&mut payer_call_bytes.as_slice())?),
            paid: dbg!(T::decode(&mut paid_call_bytes.as_slice())?),
        })
    }
}

#[derive(Debug, Encode, Decode)]
pub enum PayableCall<T> {
    Paid(PaidCall<T>),
    Unpaid(T),
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T> Send for PayableCall<T> {}

impl<T> Call for PayablePlugin<T>
where
    T: Call + State,
{
    type Call = PayableCall<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        Context::remove::<Paid>();
        match call {
            PayableCall::Unpaid(call) => self.inner.call(call),
            PayableCall::Paid(calls) => {
                let ctx = Paid {
                    running_payer: true,
                    ..Default::default()
                };
                Context::add(ctx);
                self.inner.call(calls.payer)?;

                let ctx = self.context::<Paid>().unwrap();
                ctx.running_payer = false;
                self.inner.call(calls.paid)?;
                Ok(())
            }
        }
    }
}

impl<T> ConvertSdkTx for PayablePlugin<T>
where
    T: State + ConvertSdkTx<Output = PaidCall<T::Call>> + Call,
{
    type Output = PayableCall<T::Call>;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<PayableCall<T::Call>> {
        let paid_call = self.inner.convert(sdk_tx)?;
        Ok(PayableCall::Paid(paid_call))
    }
}

pub struct UnpaidAdapter<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<fn() -> T>,
}

unsafe impl<T, U: Send + Clone> Send for UnpaidAdapter<T, U> {}

impl<T, U: Clone> Clone for UnpaidAdapter<T, U> {
    fn clone(&self) -> Self {
        UnpaidAdapter {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Call, U: AsyncCall<Call = PayableCall<T::Call>> + Clone> AsyncCall for UnpaidAdapter<T, U>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        let res = self.parent.call(PayableCall::Unpaid(call));

        res.await
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query + State,
        U: for<'a> AsyncQuery<Query = T::Query, Response<'a> = std::rc::Rc<PayablePlugin<T>>> + Clone,
    > AsyncQuery for UnpaidAdapter<T, U>
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

pub struct PaidAdapter<T, U: Clone> {
    payer_call: Vec<u8>,
    parent: U,
    marker: std::marker::PhantomData<fn() -> T>,
}

unsafe impl<T, U: Send + Clone> Send for PaidAdapter<T, U> {}

impl<T, U: Clone> Clone for PaidAdapter<T, U> {
    fn clone(&self) -> Self {
        PaidAdapter {
            payer_call: self.payer_call.clone(),
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query + State,
        U: for<'a> AsyncQuery<Query = T::Query, Response<'a> = std::rc::Rc<PayablePlugin<T>>> + Clone,
    > AsyncQuery for PaidAdapter<T, U>
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

#[async_trait::async_trait(?Send)]
impl<T: Call, U: AsyncCall<Call = PayableCall<T::Call>> + Clone> AsyncCall for PaidAdapter<T, U>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        let res = self.parent.call(PayableCall::Paid(PaidCall {
            payer: Decode::decode(self.payer_call.clone().as_slice())?,
            paid: call,
        }));

        res.await
    }
}

pub struct PayableClient<T: Client<UnpaidAdapter<T, U>>, U: Clone + Send> {
    inner: T::Client,
    parent: U,
}

pub struct PayerAdapter<T: Call> {
    intercepted_call: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    marker: std::marker::PhantomData<fn() -> T>,
}

unsafe impl<T: Call> Send for PayerAdapter<T> {}

impl<T: Call> Clone for PayerAdapter<T> {
    fn clone(&self) -> Self {
        PayerAdapter {
            intercepted_call: self.intercepted_call.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Call> AsyncCall for PayerAdapter<T>
where
    T::Call: Send,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        self.intercepted_call
            .lock()
            .unwrap()
            .replace(call.encode()?);
        Ok(())
    }
}

impl<
        T: Client<UnpaidAdapter<T, U>> + Client<PaidAdapter<T, U>> + Client<PayerAdapter<T>> + Call,
        U: Clone + Send,
    > PayableClient<T, U>
where
    <T as Client<UnpaidAdapter<T, U>>>::Client: Clone,
    <T as Client<PaidAdapter<T, U>>>::Client: Clone,
    <T as Client<PayerAdapter<T>>>::Client: Clone,
{
    pub fn pay_from<F, X: std::future::Future>(
        &mut self,
        get_payer: F,
    ) -> <T as Client<PaidAdapter<T, U>>>::Client
    where
        F: FnOnce(<T as Client<PayerAdapter<T>>>::Client) -> X,
    {
        let payer_adapter = PayerAdapter {
            intercepted_call: std::sync::Arc::new(std::sync::Mutex::new(None)),
            marker: std::marker::PhantomData,
        };
        let inner_client = T::create_client(payer_adapter.clone());

        futures_lite::future::block_on(get_payer(inner_client));

        let paid_adapter = PaidAdapter {
            payer_call: payer_adapter
                .intercepted_call
                .lock()
                .unwrap()
                .take()
                .expect("Must make payer call")
                .encode()
                .expect("Failed to encode call"),
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        };

        T::create_client(paid_adapter)
    }
}

impl<T: Client<UnpaidAdapter<T, U>>, U: Clone + Send> Clone for PayableClient<T, U>
where
    T::Client: Clone,
{
    fn clone(&self) -> Self {
        PayableClient {
            inner: self.inner.clone(),
            parent: self.parent.clone(),
        }
    }
}

impl<T: Client<UnpaidAdapter<T, U>>, U: Clone + Send> Deref for PayableClient<T, U>
where
    T::Client: Clone,
{
    type Target = T::Client;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Client<UnpaidAdapter<T, U>>, U: Clone + Send> DerefMut for PayableClient<T, U>
where
    T::Client: Clone,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T: Client<UnpaidAdapter<T, U>>, U: Clone + Send> Send for PayableClient<T, U> {}

impl<T: Client<UnpaidAdapter<T, U>> + State, U: Clone + Send> Client<U> for PayablePlugin<T>
where
    T::Client: Clone,
{
    type Client = PayableClient<T, U>;

    fn create_client(parent: U) -> Self::Client {
        PayableClient {
            inner: T::create_client(UnpaidAdapter {
                parent: parent.clone(),
                marker: std::marker::PhantomData,
            }),
            parent,
        }
    }
}

#[cfg(feature = "abci")]
mod abci {
    use super::super::*;
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<T> BeginBlock for PayablePlugin<T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T> EndBlock for PayablePlugin<T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T> InitChain for PayablePlugin<T>
    where
        T: InitChain + State + Call,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }

    impl<T> crate::abci::AbciQuery for PayablePlugin<T>
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
