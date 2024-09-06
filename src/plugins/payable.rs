//! Call-pairs with shared context for loading and deducting funds.
use orga_macros::orga;

use super::sdk_compat::{sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call;
use crate::coins::{Amount, Coin, Symbol};
use crate::context::{Context, GetContext};

use crate::encoding::{Decode, Encode};

use crate::state::State;
use crate::{Error, Result};
use std::collections::HashMap;
use std::convert::TryInto;

const MAX_SUBCALL_LEN: u32 = 200_000;

/// A plugin which allows a pair of calls to be issued together, with the first
/// call (`payer` call) loading funds into the [Paid] context for use in the
/// second call (`paid` call).
#[orga(skip(Call))]
pub struct PayablePlugin<T> {
    /// The inner value.
    pub inner: T,
}

/// Context for loading and deducting funds of multiple symbols.
#[derive(Default)]
pub struct Paid {
    /// Amounts of each symbol index currently in the context.
    map: HashMap<u8, Amount>,
    /// True if the current call is the payer call.
    pub running_payer: bool,
    /// Whether the fee for this call has been explicitly disabled.
    pub fee_disabled: bool,
}

impl Paid {
    /// Adds `amount` of symbol `S` to the context.
    pub fn give<S: Symbol, A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        self.give_denom(amount, S::INDEX)
    }

    /// Adds `amount` of symbol with index `denom` to the context.
    pub fn give_denom<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        let entry = self.map.entry(denom).or_insert_with(|| 0.into());
        let amount = amount.into();
        *entry = (*entry + amount)?;

        Ok(())
    }

    /// Takes `amount` of symbol `S` from the context.
    pub fn take<S: Symbol, A: Into<Amount>>(&mut self, amount: A) -> Result<Coin<S>> {
        let amount = amount.into();
        self.take_denom(amount, S::INDEX)?;

        Ok(S::mint(amount))
    }

    /// Takes `amount` of symbol with index `denom` from the context.
    pub fn take_denom<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        let entry = self.map.entry(denom).or_insert_with(|| 0.into());
        let amount = amount.into();
        if *entry < amount {
            return Err(Error::Coins("Insufficient funding for paid call".into()));
        }

        *entry = (*entry - amount)?;

        Ok(())
    }

    /// Returns the amount of symbol `S` currently in the context.
    pub fn balance<S: Symbol>(&self) -> Result<Amount> {
        let entry = match self.map.get(&S::INDEX) {
            Some(amt) => *amt,
            None => 0.into(),
        };

        Ok(entry)
    }
}

/// A two-part call, where the `payer` call may load funds into the [Paid]
/// context for use in the `paid` call.
///
/// This two-call system will be replaced by a more flexible "multi-call" system
/// in the future.
#[derive(Debug)]
pub struct PaidCall<T> {
    /// The `payer` call, which may load funds into the [Paid] context (e.g. to
    /// cover fees or deposit into another account).
    pub payer: T,
    /// The `paid` call, which may use funds loaded into the [Paid] context by
    /// the `payer` call.
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

/// A payable call, which may be either a [PaidCall] or the inner type's call
/// (unpaid).
#[derive(Debug, Encode, Decode)]
pub enum PayableCall<T> {
    /// A paid call.
    Paid(PaidCall<T>),
    /// An unpaid call, passed through to the inner value.
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
            request: &tendermint_proto::v0_34::abci::RequestQuery,
        ) -> Result<tendermint_proto::v0_34::abci::ResponseQuery> {
            self.inner.abci_query(request)
        }
    }
}
