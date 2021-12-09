use super::{BeginBlockCtx, EndBlockCtx, InitChainCtx, Signer};
use crate::abci::{BeginBlock, EndBlock, InitChain};
use crate::call::Call;
use crate::client::{AsyncCall, Client};
use crate::coins::{Address, Amount, Coin, Give, Symbol, Take};
use crate::context::{Context, GetContext};
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use std::any::TypeId;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

#[derive(State, Encode, Decode)]
pub struct PayablePlugin<T: State> {
    inner: T,
}

impl<T: State> Deref for PayablePlugin<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Default)]
pub struct Paid {
    map: HashMap<TypeId, Amount>,
}

impl Paid {
    pub fn give<S: Symbol, A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        let entry = self
            .map
            .entry(TypeId::of::<S>())
            .or_insert_with(|| 0.into());
        let amount = amount.into();
        *entry = (*entry + amount)?;

        Ok(())
    }

    pub fn take<S: Symbol, A: Into<Amount>>(&mut self, amount: A) -> Result<Coin<S>> {
        let entry = self
            .map
            .entry(TypeId::of::<S>())
            .or_insert_with(|| 0.into());
        let amount = amount.into();
        if *entry < amount {
            return Err(Error::Coins("Insufficient funding for paid call".into()));
        }

        *entry = (*entry - amount)?;

        Ok(amount.into())
    }
}

pub struct PaidCall<T> {
    payer: T,
    paid: T,
}

impl<T: Encode> Encode for PaidCall<T> {
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.payer.encoding_length()? + self.paid.encoding_length()?)
    }
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let payer_call_bytes = self.payer.encode()?;
        let payer_call_len = payer_call_bytes.len() as u16;
        let paid_call_bytes = self.paid.encode()?;
        let paid_call_len = paid_call_bytes.len() as u16;

        dest.write_all(&payer_call_len.encode()?)?;
        dest.write_all(&payer_call_bytes)?;
        dest.write_all(&paid_call_len.encode()?)?;
        dest.write_all(&paid_call_bytes)?;

        Ok(())
    }
}

impl<T: Decode> Decode for PaidCall<T> {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let payer_call_len = u16::decode(&mut reader)?;
        let mut payer_call_bytes = vec![0u8; payer_call_len as usize];
        reader.read_exact(&mut payer_call_bytes)?;
        let paid_call_len = u16::decode(&mut reader)?;
        let mut paid_call_bytes = vec![0u8; paid_call_len as usize];
        reader.read_exact(&mut paid_call_bytes)?;

        Ok(Self {
            payer: T::decode(&mut payer_call_bytes.as_slice())?,
            paid: T::decode(&mut paid_call_bytes.as_slice())?,
        })
    }
}

#[derive(Encode, Decode)]
pub enum PayableCall<T> {
    Paid(PaidCall<T>),
    Unpaid(T),
}

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
                Context::add(Paid::default());
                self.inner.call(calls.payer)?;
                let res = self.inner.call(calls.paid)?;
                Ok(res)
            }
        }
    }
}

impl<T: Query + State> Query for PayablePlugin<T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

pub struct PayableClient<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<T>,
}

impl<T, U: Clone> Clone for PayableClient<T, U> {
    fn clone(&self) -> Self {
        PayableClient {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

unsafe impl<T, U: Clone + Send> Send for PayableClient<T, U> {}

#[async_trait::async_trait]
impl<T: Call, U: AsyncCall<Call = PayableCall<T>> + Clone> AsyncCall for PayableClient<T, U>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        // self.parent.call(PayableCall::Unpaid(call)).await
        todo!()
    }
}

impl<T: Client<PayableClient<T, U>> + State, U: Clone> Client<U> for PayablePlugin<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(PayableClient {
            parent,
            marker: std::marker::PhantomData,
        })
    }
}

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
