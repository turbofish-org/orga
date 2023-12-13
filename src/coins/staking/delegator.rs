use crate::coins::{Amount, Balance, Coin, Decimal, Give, Share, Symbol, Take};
use crate::coins::{MultiShare, VersionedAddress as Address};
use crate::collections::Deque;
use crate::context::GetContext;
use crate::orga;
use crate::plugins::Time;
use crate::{Error, Result};

use super::UNBONDING_SECONDS;

#[orga]
pub struct Unbond<S: Symbol> {
    pub coins: Share<S>,
    pub start_seconds: i64,
}

#[orga]
#[derive(Clone)]
pub struct Redelegation {
    pub amount: Amount,
    pub address: Address,
    pub start_seconds: i64,
}

#[orga]
pub struct Delegator<S: Symbol> {
    pub liquid: MultiShare,
    pub staked: Share<S>,
    pub unbonding: Deque<Unbond<S>>,
    pub redelegations_out: Deque<Redelegation>,
    pub redelegations_in: Deque<Redelegation>,
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
            self.unbonding.push_back(unbond)
        } else {
            self.liquid.give(S::mint(amount))
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
            liquid: self.liquid.amounts()?,
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

    pub(super) fn slash_redelegation(&mut self, amount: Amount) -> Result<()> {
        let stake_slash = if amount > self.staked.shares.amount()? {
            self.staked.shares.amount()?
        } else {
            amount
        };

        if stake_slash > 0 {
            self.staked.take(stake_slash)?.burn();
        }

        if stake_slash == amount {
            return Ok(());
        }

        let mut remaining_slash = (amount - stake_slash)?;

        for i in 0..self.unbonding.len() {
            let unbond = self.unbonding.get_mut(i)?;
            if let Some(mut unbond) = unbond {
                let unbond_slash = if remaining_slash > unbond.coins.shares.amount()? {
                    unbond.coins.shares.amount()?
                } else {
                    remaining_slash
                };
                if unbond_slash > 0 {
                    unbond.coins.take(unbond_slash)?.burn();
                }
                remaining_slash = (remaining_slash - unbond_slash)?;

                if remaining_slash == 0 {
                    break;
                }
            }
        }

        Ok(())
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
                self.liquid.give(S::mint(unbond.coins.shares.amount()?))?;
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
        start_seconds: Option<i64>,
    ) -> Result<Coin<S>> {
        if !self.redelegations_in.is_empty() {
            return Err(Error::Coins(
                "Cannot redelegate from validator with inbound redelegations".into(),
            ));
        }

        let redelegated_coins = self.staked.take(amount)?;
        if let Some(start_seconds) = start_seconds {
            self.redelegations_out.push_back(Redelegation {
                amount,
                address: dst_val_address,
                start_seconds,
            })?;
        }

        Ok(redelegated_coins)
    }

    pub(super) fn redelegate_in(
        &mut self,
        src_val_address: Address,
        coins: Coin<S>,
        start_seconds: Option<i64>,
    ) -> Result<()> {
        if let Some(start_seconds) = start_seconds {
            self.redelegations_in.push_back(Redelegation {
                address: src_val_address,
                amount: coins.amount,
                start_seconds,
            })?;
        }

        self.add_stake(coins)
    }

    pub(super) fn add_stake(&mut self, coins: Coin<S>) -> Result<()> {
        self.staked.give(coins)
    }

    pub(super) fn deduct<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        self.liquid.deduct(amount.into(), denom)
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

impl<S: Symbol> Give<(u8, Amount)> for Delegator<S> {
    fn give(&mut self, coins: (u8, Amount)) -> Result<()> {
        self.liquid.give(coins)
    }
}

#[orga]
#[derive(Debug)]
pub struct UnbondInfo {
    pub start_seconds: i64,
    pub amount: Amount,
}

#[derive(Debug)]
pub struct DelegationInfo {
    pub unbonding: Vec<UnbondInfo>,
    pub staked: Amount,
    pub liquid: Vec<(u8, Amount)>,
}
