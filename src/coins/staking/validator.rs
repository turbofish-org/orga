use crate::coins::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use crate::coins::{Address, Amount, Balance, Coin, Decimal, Give, Pool, Symbol, VersionedAddress};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode, LengthVec};
use crate::orga;
use crate::plugins::Time;
use crate::{Error, Result};

use super::{Commission, Delegator, Redelegation};

/// [Pool] of [Delegator] indexed by validator [Address]
type Delegators<S> = Pool<Address, Delegator<S>, S>;

/// A single declared validator.
///
/// A `Validator` tracks the staking state of a single validator, including its
/// commission rate, delegators, and unbonding status.
#[orga]
pub struct Validator<S: Symbol> {
    /// If the validator is jailed, the time (in unix seconds) when it will be
    /// eligible to unjail.
    pub(super) jailed_until: Option<i64>,
    /// Whether the validator may re-enter the active set.
    pub(super) tombstoned: bool,
    /// Operator address.
    pub(super) address: VersionedAddress,
    /// Commission settings.
    pub(super) commission: Commission,
    /// Delegators staked to this validator.
    pub(super) delegators: Delegators<S>,
    /// Metadata used for display purposes. Not parsed on-chain.
    pub(super) info: ValidatorInfo,
    /// Whether this validator is currently in the active set.
    pub(super) in_active_set: bool,
    /// Whether this validator is currently unbonding.
    pub(super) unbonding: bool,
    /// When this validator started unbonding, if applicable.
    pub(super) unbonding_start_seconds: i64,
    /// The last time this validator was edited, in unix seconds.
    pub(super) last_edited_seconds: i64,
    /// The minimum amount this validator must keep self-delegated to remain
    pub(super) min_self_delegation: Amount,
}

/// Queryable information about a validator, aggregated for convenience.
#[derive(Encode, Decode)]
pub struct ValidatorQueryInfo {
    /// If the validator is jailed, the time (in unix seconds) when it will be
    /// eligible to unjail.
    pub jailed_until: Option<i64>,
    /// Whether the validator may re-enter the active set.
    pub tombstoned: bool,
    /// Operator address.
    pub address: VersionedAddress,
    /// Commission settings.
    pub commission: Commission,
    /// Metadata used for display purposes. Not parsed on-chain.
    pub info: ValidatorInfo,
    /// Whether the validator is currently in the active set.
    pub in_active_set: bool,
    /// Whether the validator is currently unbonding.
    pub unbonding: bool,
    /// When the validator started unbonding, if applicable.
    pub unbonding_start_seconds: i64,
    /// The minimum amount this validator must keep self-delegated to remain
    /// active.
    pub min_self_delegation: Amount,
    /// Whether the validator is currently jailed.
    pub jailed: bool,
    /// The total amount staked to this validator.
    pub amount_staked: Amount,
}

/// Metadata used for display purposes. Not parsed on-chain.
pub type ValidatorInfo = LengthVec<u16, u8>;

/// Current validator status, computed by [Validator::status]
#[derive(Encode, Decode)]
pub enum Status {
    /// Inactive, tokens may be freely unbonded.
    Unbonded,
    /// Active, tokens are staked and participating in consensus. Unbonds are
    /// subject to the full unbonding period.
    Bonded,
    /// Inactive, but tokens are still at stake for past actions. Unbonds are
    /// initialized with the time this status was entered.
    Unbonding {
        /// The time (in unix seconds) when the unbonding started.
        start_seconds: i64,
    },
}

/// Data required to slash redelegations from a single DVP.
pub(super) struct SlashableRedelegation {
    /// Delegator address
    pub delegator_address: VersionedAddress,
    /// Outbound redelegations that may be slashed.
    pub outbound_redelegations: Vec<Redelegation>,
}

impl<S: Symbol + Default> Validator<S> {
    /// Returns a [PoolChildMut] for the given delegator address, resolving
    /// mutations efficiently on drop.
    pub(super) fn get_mut(
        &mut self,
        address: Address,
    ) -> Result<PoolChildMut<Address, Delegator<S>, S>> {
        self.delegators.get_mut(address)
    }

    /// Returns a [PoolChild] for the given delegator address, ensuring
    /// correctness of the [Delegator] state on deref.
    pub fn get(&self, address: Address) -> Result<PoolChild<Delegator<S>, S>> {
        self.delegators.get(address)
    }

    /// How much voting power the validator would have, were they in the active
    /// set.
    pub fn potential_vp(&mut self) -> Result<Amount> {
        let in_active_set_before = self.in_active_set;
        self.in_active_set = true;
        let res = self.balance()?.amount();
        self.in_active_set = in_active_set_before;

        res
    }

    /// Amount staked to this validator.
    pub fn staked(&self) -> Result<Amount> {
        self.balance()?.amount()
    }

    /// Whether this validator is currently jailed.
    pub fn jailed(&self) -> bool {
        self.jailed_until.is_some()
    }

    /// The current bonding status of the validator.
    pub fn status(&self) -> Status {
        if self.unbonding {
            Status::Unbonding {
                start_seconds: self.unbonding_start_seconds,
            }
        } else if self.in_active_set {
            Status::Bonded
        } else {
            Status::Unbonded
        }
    }

    /// Jails the validator for a given number of seconds.
    pub(super) fn jail_for_seconds(&mut self, seconds: u64) -> Result<()> {
        let now = self.current_seconds()?;
        let jailed_until = match self.jailed_until {
            Some(jailed_until) => (now + seconds as i64).max(jailed_until),
            None => now + seconds as i64,
        };

        self.jailed_until.replace(jailed_until);

        Ok(())
    }

    /// Jails the validator indefinitely.
    pub(super) fn jail_forever(&mut self) {
        self.jailed_until.replace(i64::MAX);
    }

    /// Unjails the validator if it is currently jailed and eligible to unjail.
    pub(super) fn try_unjail(&mut self) -> Result<()> {
        match self.jailed_until {
            Some(jailed_until) => {
                let now = self.current_seconds()?;
                if now > jailed_until {
                    self.jailed_until = None;
                } else {
                    return Err(Error::Coins("Validator cannot yet unjail".into()));
                }
            }
            None => return Err(Error::Coins("Validator is not jailed".into())),
        }

        Ok(())
    }

    /// Slash all funds staked to the validator by the given `penalty`.
    pub(super) fn slash(
        &mut self,
        penalty: Decimal,
        liveness_fault: bool,
    ) -> Result<Vec<SlashableRedelegation>> {
        if self.tombstoned {
            return Ok(vec![]);
        }
        if !liveness_fault {
            self.tombstoned = true;
        }
        let slash_multiplier = (Decimal::one() - penalty)?;
        let delegator_keys = self.delegator_keys()?;
        let mut redelegations = vec![];
        delegator_keys.iter().try_for_each(|k| -> Result<()> {
            let mut delegator = self.get_mut(*k)?;
            let slashable_redelegations = delegator.slash(slash_multiplier, liveness_fault)?;
            redelegations.push(SlashableRedelegation {
                delegator_address: (*k).into(),
                outbound_redelegations: slashable_redelegations,
            });
            Ok(())
        })?;

        Ok(redelegations)
    }

    /// Returns all addresses delegated to this validator.
    pub fn delegator_keys(&self) -> Result<Vec<Address>> {
        let mut delegator_keys: Vec<Address> = vec![];
        self.delegators
            .iter()?
            .try_for_each(|entry| -> Result<()> {
                let (k, _v) = entry?;
                delegator_keys.push(k);

                Ok(())
            })?;

        Ok(delegator_keys)
    }

    /// Returns a [ValidatorQueryInfo] for this validator.
    pub(super) fn query_info(&self) -> Result<ValidatorQueryInfo> {
        Ok(ValidatorQueryInfo {
            jailed_until: self.jailed_until,
            address: self.address,
            commission: self.commission,
            in_active_set: self.in_active_set,
            info: self.info.clone(),
            min_self_delegation: self.min_self_delegation,
            tombstoned: self.tombstoned,
            unbonding: self.unbonding,
            unbonding_start_seconds: self.unbonding_start_seconds,

            jailed: self.jailed(),
            amount_staked: self.delegators.balance()?.amount()?,
        })
    }

    /// Returns the current time in unix seconds.
    fn current_seconds(&mut self) -> Result<i64> {
        let time = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?;

        Ok(time.seconds)
    }

    /// Returns the self-delegation amount of the validator.
    pub(super) fn self_delegation(&self) -> Result<Amount> {
        self.delegators.get(self.address.into())?.staked.amount()
    }

    /// Checks whether the validator is below their required self-delegation.
    fn below_required_self_delegation(&self) -> Result<bool> {
        Ok(self.self_delegation()? < self.min_self_delegation)
    }
}

impl<S: Symbol> Balance<S, Decimal> for Validator<S> {
    fn balance(&self) -> Result<Decimal> {
        if self.jailed() || !self.in_active_set || self.below_required_self_delegation()? {
            Ok(0.into())
        } else {
            self.delegators.balance()
        }
    }
}

impl<S: Symbol, T: Symbol> Give<Coin<T>> for Validator<S> {
    fn give(&mut self, coins: Coin<T>) -> Result<()> {
        let one: Decimal = 1.into();
        let delegator_amount = (coins.amount * (one - self.commission.rate))?.amount()?;
        let validator_amount = (coins.amount * self.commission.rate)?.amount()?;

        self.delegators.give(T::mint(delegator_amount))?;
        self.delegators
            .get_mut(self.address.into())?
            .give((T::INDEX, validator_amount))?;

        Ok(())
    }
}

impl<S: Symbol> Give<(u8, Amount)> for Validator<S> {
    fn give(&mut self, coins: (u8, Amount)) -> Result<()> {
        let one: Decimal = 1.into();
        let delegator_amount = (coins.1 * (one - self.commission.rate))?.amount()?;
        let validator_amount = (coins.1 * self.commission.rate)?.amount()?;

        self.delegators.give((coins.0, delegator_amount))?;
        self.delegators
            .get_mut(self.address.into())?
            .give((coins.0, validator_amount))?;

        Ok(())
    }
}
