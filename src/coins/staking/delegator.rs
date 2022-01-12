use crate::coins::{Amount, Balance, Coin, Decimal, Give, Share, Symbol, Take};
use crate::collections::Deque;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
#[cfg(feature = "abci")]
use crate::plugins::Time;
use crate::state::State;
use crate::{Error, Result};

use super::UNBONDING_SECONDS;

#[derive(State)]
pub struct Unbond<S: Symbol> {
    pub(super) coins: Share<S>,
    pub(super) start_seconds: i64,
}
#[derive(State)]
pub struct Delegator<S: Symbol> {
    pub(super) liquid: Share<S>,
    pub(super) staked: Share<S>,
    pub(super) jailed: bool,
    pub(super) unbonding: Deque<Unbond<S>>,
    pub(super) multiplier: Decimal,
}

impl<S: Symbol> Delegator<S> {
    pub(super) fn unbond<A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        let amount = amount.into();
        let coins = self.staked.take(amount)?.into();

        #[cfg(feature = "abci")]
        let start_seconds = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?
            .seconds;
        #[cfg(not(feature = "abci"))]
        unimplemented!();
        #[cfg(not(feature = "abci"))]
        let start_seconds = 0;

        let unbond = Unbond {
            coins,
            start_seconds,
        };
        self.unbonding.push_back(unbond.into())?;

        Ok(())
    }

    pub(super) fn info(&self) -> Result<DelegationInfo> {
        Ok(DelegationInfo {
            liquid: self.liquid.shares.amount()?,
            staked: self.staked.shares.amount()?,
        })
    }

    pub(super) fn slashable_balance(&mut self) -> Result<Decimal> {
        self.process_unbonds()?;
        let mut sum: Decimal = 0.into();
        sum = (sum + self.staked.shares)?;
        for i in 0..self.unbonding.len() {
            let unbond = self
                .unbonding
                .get(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;
            sum = (sum + unbond.coins.shares)?;
        }

        Ok(sum)
    }

    pub(super) fn slash(&mut self, multiplier: Decimal) -> Result<()> {
        self.staked.shares = (self.staked.shares * multiplier)?;
        for i in 0..self.unbonding.len() {
            let mut unbond = self
                .unbonding
                .get_mut(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;
            unbond.coins.shares = (unbond.coins.shares * multiplier)?;
        }
        self.jailed = true;
        Ok(())
    }

    pub(super) fn process_unbonds(&mut self) -> Result<()> {
        #[cfg(feature = "abci")]
        let now_seconds = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?
            .seconds;
        #[cfg(not(feature = "abci"))]
        unimplemented!();
        #[cfg(not(feature = "abci"))]
        let now_seconds = 0;

        while let Some(unbond) = self.unbonding.front()? {
            let unbond_matured = now_seconds - unbond.start_seconds >= UNBONDING_SECONDS as i64;
            if unbond_matured {
                let unbond = self
                    .unbonding
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Failed to pop unbond".into()))?;
                self.liquid.add(unbond.coins.shares.amount()?)?;
            } else {
                break;
            }
        }

        Ok(())
    }

    pub(super) fn add_stake(&mut self, coins: Coin<S>) -> Result<()> {
        self.staked.give(coins)
    }

    pub(super) fn withdraw_liquid<A: Into<Amount>>(&mut self, amount: A) -> Result<Coin<S>> {
        self.liquid.take(amount.into())
    }
}

/// A delegator's balance is its staked coins, since Balance here is used in the
/// parent collection to calculate the validator's voting power.
impl<S: Symbol> Balance<S, Decimal> for Delegator<S> {
    fn balance(&self) -> Result<Decimal> {
        if self.jailed {
            Ok(0.into())
        } else {
            Ok(self.staked.shares)
        }
    }
}

impl<S: Symbol> Give<S> for Delegator<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        self.liquid.give(coins)
    }
}

#[derive(Encode, Decode)]
pub struct DelegationInfo {
    pub staked: Amount,
    pub liquid: Amount,
}
