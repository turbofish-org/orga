use crate::coins::{Amount, Balance, Coin, Decimal, Give, Share, Symbol, Take};
use crate::coins::{MultiShare, VersionedAddress as Address};
use crate::collections::Deque;
use crate::context::GetContext;
use crate::orga;
use crate::plugins::Time;
use crate::{Error, Result};

use super::UNBONDING_SECONDS;

/// Unbonding entry of staked to liquid coins.
#[orga]
pub struct Unbond<S: Symbol> {
    /// The funds being unbonded.
    pub coins: Share<S>,
    /// The time (in unix seconds) when the unbonding started.
    pub start_seconds: i64,
}

/// Redelegation entry of staked coins from one validator to another.
#[orga]
#[derive(Clone)]
pub struct Redelegation {
    /// The amount of the staking token being redelegated.
    pub amount: Amount,
    /// The validator address to which the staking token is being redelegated.
    pub address: Address,
    /// The time (in unix seconds) when the redelegation started.
    pub start_seconds: i64,
}

/// A single delegator entry within a validator, aka a Delegator Validator Pair
/// (DVP).
#[orga]
pub struct Delegator<S: Symbol> {
    /// Claimable staking rewards, which may include multiple denoms.
    pub liquid: MultiShare,
    /// Amount of staking token staked by the delegator.
    pub staked: Share<S>,
    /// Queue of unbonds for this DVP.
    pub unbonding: Deque<Unbond<S>>,
    /// Queue of outgoing redelegations from this DVP.
    pub redelegations_out: Deque<Redelegation>,
    /// Queue of incoming redelegations to this DVP.
    pub redelegations_in: Deque<Redelegation>,
}

impl<S: Symbol> Delegator<S> {
    /// Begin an unbond. If no start time is provided, the unbonded coins will
    /// be liquid immediately.
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

    /// Build a summary of the delegator's staking state.
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

    /// Slash the stake of this delegator by the given multiplier, and return
    /// any redelegations also subject to the slash.
    ///
    /// If the slash is due to a liveness fault, outbound
    /// redelegations are not affected.
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

    /// Slash a redelation by the given amount.
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

    /// Process matured unbonds.
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

    /// Process matured redelegations with this DVP as their destination.
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

    /// Process matured redelegations with this DVP as their source.
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

    /// Initialize a redelegation away from this DVP.
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

    /// Initialize a redelegation to this DVP.
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

    /// Add staked coins to this delegator.
    pub(super) fn add_stake(&mut self, coins: Coin<S>) -> Result<()> {
        self.staked.give(coins)
    }

    /// Deduct staked coins from this delegator.
    pub(super) fn deduct<A: Into<Amount>>(&mut self, amount: A, denom: u8) -> Result<()> {
        self.liquid.deduct(amount.into(), denom)
    }

    /// Time context helper.
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

/// Information about a single unbond of the staking token.
#[orga]
#[derive(Debug)]
pub struct UnbondInfo {
    /// The time (in unix seconds) when the unbonding started.
    pub start_seconds: i64,
    /// Amount of the staking token being unbonded.
    pub amount: Amount,
}

/// Summary of a delegator for a single DVP.
#[derive(Debug)]
pub struct DelegationInfo {
    /// Pending unbonds.
    pub unbonding: Vec<UnbondInfo>,
    /// Total amount staked.
    pub staked: Amount,
    /// Claimable staking rewards, in (denom, amount) pairs.
    pub liquid: Vec<(u8, Amount)>,
}
