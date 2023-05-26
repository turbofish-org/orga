use super::sdk_compat::{sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call;
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

#[orga_macros::orga(skip(MigrateFrom, Call))]
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
        let payer_call_bytes = self.payer.encode()?;
        let payer_call_len: u32 = payer_call_bytes
            .len()
            .try_into()
            .map_err(|_| ed::Error::UnexpectedByte(0))?;
        let paid_call_bytes = self.paid.encode()?;
        let paid_call_len: u32 = paid_call_bytes
            .len()
            .try_into()
            .map_err(|_| ed::Error::UnexpectedByte(0))?;

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
            payer: T::decode(&mut payer_call_bytes.as_slice())?,
            paid: T::decode(&mut paid_call_bytes.as_slice())?,
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
