use crate::coins::Address;
use crate::coins::{Amount, Balance, Coin, Decimal, Give, Share, Symbol, Take};
use crate::collections::Deque;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::Time;
use crate::state::State;
use crate::{Error, Result};

use super::UNBONDING_SECONDS;

#[derive(State)]
pub struct Unbond<S: Symbol> {
    pub(super) coins: Share<S>,
    pub(super) start_seconds: i64,
}

#[derive(State, Clone)]
pub struct Redelegation {
    pub(super) amount: Amount,
    pub(super) address: Address,
    pub(super) start_seconds: i64,
}

#[derive(State)]
pub struct Delegator<S: Symbol> {
    pub(super) liquid: Share<S>,
    pub(super) staked: Share<S>,
    pub(super) unbonding: Deque<Unbond<S>>,
    pub(super) redelegations_out: Deque<Redelegation>,
    pub(super) redelegations_in: Deque<Redelegation>,
}

impl<S: Symbol> Delegator<S> {
    pub(super) fn unbond<A: Into<Amount>>(
        &mut self,
        amount: A,
        start_seconds: Option<i64>,
    ) -> Result<()> {
        let amount = amount.into();
        let coins = self.staked.take(amount)?.into();
        if let Some(start_seconds) = start_seconds {
            let unbond = Unbond {
                coins,
                start_seconds,
            };
            self.unbonding.push_back(unbond.into())
        } else {
            self.liquid.give(amount.into())
        }
    }

    pub(super) fn info(&self) -> Result<DelegationInfo> {
        let mut unbonds = vec![];
        for i in 0..self.unbonding.len() {
            if let Some(unbond) = self.unbonding.get(i)? {
                unbonds.push(UnbondInfo {
                    start_seconds: unbond.start_seconds,
                    amount: unbond.coins.amount()?,
                });
            }
        }

        Ok(DelegationInfo {
            unbonding: unbonds,
            liquid: self.liquid.shares.amount()?,
            staked: self.staked.shares.amount()?,
        })
    }

    pub(super) fn slash(
        &mut self,
        multiplier: Decimal,
        liveness_fault: bool,
    ) -> Result<Vec<Redelegation>> {
        self.staked.shares = (self.staked.shares * multiplier)?;
        if liveness_fault {
            return Ok(vec![]);
        }
        for i in 0..self.unbonding.len() {
            let mut unbond = self
                .unbonding
                .get_mut(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;

            unbond.coins.shares = (unbond.coins.shares * multiplier)?;
        }

        let mut redelegations = vec![];
        for i in 0..self.redelegations_out.len() {
            let redelegation = self
                .redelegations_out
                .get(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over redelegations".into()))?;
            redelegations.push(redelegation.clone());
        }

        Ok(redelegations)
    }

    pub(super) fn process_unbonds(&mut self) -> Result<()> {
        let now = self.current_seconds()?;

        while let Some(unbond) = self.unbonding.front()? {
            let unbond_matured = now - unbond.start_seconds >= UNBONDING_SECONDS as i64;
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

    pub(super) fn process_redelegations_in(&mut self) -> Result<()> {
        let now = self.current_seconds()?;
        while let Some(redelegation) = self.redelegations_in.front()? {
            let matured = now - redelegation.start_seconds >= UNBONDING_SECONDS as i64;
            if matured {
                self.redelegations_in
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Failed to pop inbound redelegation".into()))?;
            } else {
                break;
            }
        }

        Ok(())
    }

    pub(super) fn process_redelegations_out(&mut self) -> Result<()> {
        let now = self.current_seconds()?;
        while let Some(redelegation) = self.redelegations_out.front()? {
            let matured = now - redelegation.start_seconds >= UNBONDING_SECONDS as i64;
            if matured {
                self.redelegations_out
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Failed to pop outbound redelegation".into()))?;
            } else {
                break;
            }
        }

        Ok(())
    }

    pub(super) fn redelegate_out(
        &mut self,
        dst_val_address: Address,
        amount: Amount,
        start_seconds: i64,
    ) -> Result<Coin<S>> {
        if !self.redelegations_in.is_empty() {
            return Err(Error::Coins(
                "Cannot redelegate from validator with inbound redelegations".into(),
            ));
        }

        let redelegated_coins = self.staked.take(amount)?;

        self.redelegations_out.push_back(
            Redelegation {
                amount,
                address: dst_val_address,
                start_seconds,
            }
            .into(),
        )?;

        Ok(redelegated_coins)
    }

    pub(super) fn redelegate_in(
        &mut self,
        src_val_address: Address,
        coins: Coin<S>,
        start_seconds: i64,
    ) -> Result<()> {
        self.redelegations_in.push_back(
            Redelegation {
                address: src_val_address,
                amount: coins.amount,
                start_seconds,
            }
            .into(),
        )?;

        self.add_stake(coins)
    }

    pub(super) fn add_stake(&mut self, coins: Coin<S>) -> Result<()> {
        self.staked.give(coins)
    }

    pub(super) fn withdraw_liquid<A: Into<Amount>>(&mut self, amount: A) -> Result<Coin<S>> {
        self.liquid.take(amount.into())
    }

    fn current_seconds(&mut self) -> Result<i64> {
        Ok(self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?
            .seconds)
    }
}

/// A delegator's balance is its staked coins, since Balance here is used in the
/// parent collection to calculate the validator's voting power.
impl<S: Symbol> Balance<S, Decimal> for Delegator<S> {
    fn balance(&self) -> Result<Decimal> {
        Ok(self.staked.shares)
    }
}

impl<S: Symbol> Give<S> for Delegator<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        self.liquid.give(coins)
    }
}

#[derive(Encode, Decode)]
pub struct UnbondInfo {
    pub start_seconds: i64,
    pub amount: Amount,
}

#[derive(Encode, Decode)]
pub struct DelegationInfo {
    pub unbonding: Vec<UnbondInfo>,
    pub staked: Amount,
    pub liquid: Amount,
}
