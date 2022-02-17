use crate::coins::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use crate::coins::{Address, Amount, Balance, Coin, Decimal, Give, Pool, Symbol};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::Time;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use ed::Terminated;

use super::Delegator;

type Delegators<S> = Pool<Address, Delegator<S>, S>;

#[derive(State)]
pub struct Validator<S: Symbol> {
    pub(super) jailed: bool,
    pub(super) jailed_until: i64,
    pub(super) address: Address,
    pub(super) commission: Commission,
    pub(super) delegators: Delegators<S>,
    pub(super) info: ValidatorInfo,
    pub(super) in_active_set: bool,
    pub(super) unbonding: bool,
    pub(super) unbonding_start_seconds: i64,
    pub(super) last_edited_seconds: i64,
    pub(super) min_self_delegation: Amount,
    pub(super) self_delegation: Amount,
}

#[derive(State, Default)]
pub struct Commission {
    pub rate: Decimal,
    pub max: Decimal,
    pub max_change: Decimal,
}

#[derive(Encode, Decode)]
pub struct ValidatorQueryInfo {
    pub jailed: bool,
    pub address: Address,
    pub commission: Decimal,
    pub in_active_set: bool,
    pub info: ValidatorInfo,
    pub amount_staked: Amount,
}

#[derive(Default, Clone)]
pub struct ValidatorInfo {
    pub bytes: Vec<u8>,
}

impl From<Vec<u8>> for ValidatorInfo {
    fn from(bytes: Vec<u8>) -> Self {
        ValidatorInfo { bytes }
    }
}

impl Encode for ValidatorInfo {
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.bytes.len() + 2)
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let info_byte_len = self.bytes.len() as u16;

        dest.write_all(&info_byte_len.encode()?)?;
        dest.write_all(&self.bytes)?;

        Ok(())
    }
}

impl Terminated for ValidatorInfo {}

impl Decode for ValidatorInfo {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let info_byte_len = u16::decode(&mut reader)?;
        let mut bytes = vec![0u8; info_byte_len as usize];
        reader.read_exact(&mut bytes)?;

        Ok(ValidatorInfo { bytes })
    }
}

impl State for ValidatorInfo {
    type Encoding = Self;

    fn create(_store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(data)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}
impl From<ValidatorInfo> for Vec<u8> {
    fn from(info: ValidatorInfo) -> Self {
        info.bytes
    }
}

#[derive(Encode, Decode)]
pub(super) enum Status {
    Unbonded,
    Bonded,
    Unbonding { start_seconds: i64 },
}

impl<S: Symbol> Validator<S> {
    pub(super) fn get_mut(
        &mut self,
        address: Address,
    ) -> Result<PoolChildMut<Address, Delegator<S>, S>> {
        self.delegators.get_mut(address)
    }

    pub fn get(&self, address: Address) -> Result<PoolChild<Delegator<S>, S>> {
        self.delegators.get(address)
    }

    pub fn staked(&mut self) -> Result<Amount> {
        let in_active_set_before = self.in_active_set;
        self.in_active_set = true;
        let res = self.balance()?.amount();
        self.in_active_set = in_active_set_before;

        res
    }

    pub(super) fn status(&self) -> Status {
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

    pub(super) fn jail_for_seconds(&mut self, seconds: u64) -> Result<()> {
        self.jailed = true;
        let now = self.current_seconds()?;
        self.jailed_until = (now + seconds as i64).max(self.jailed_until);

        Ok(())
    }

    pub(super) fn jail_forever(&mut self) {
        self.jailed = true;
        self.jailed_until = i64::MAX;
    }

    pub(super) fn try_unjail(&mut self) -> Result<()> {
        if !self.jailed {
            return Err(Error::Coins("Validator is not jailed".into()));
        }
        let now = self.current_seconds()?;
        if now > self.jailed_until {
            self.jailed = false;
            Ok(())
        } else {
            Err(Error::Coins("Validator cannot yet unjail".into()))
        }
    }

    pub(super) fn slash(&mut self, penalty: Decimal) -> Result<()> {
        let one: Decimal = 1.into();
        let slash_multiplier = (one - penalty)?;
        let delegator_keys = self.delegator_keys()?;
        delegator_keys.iter().try_for_each(|k| -> Result<()> {
            let mut delegator = self.get_mut(*k)?;
            delegator.slash(slash_multiplier)?;
            Ok(())
        })?;

        self.update_self_delegation()
    }

    pub fn slashable_balance(&mut self) -> Result<Amount> {
        let mut sum: Decimal = 0.into();
        let delegator_keys = self.delegator_keys()?;
        delegator_keys.iter().try_for_each(|k| -> Result<_> {
            let mut delegator = self.get_mut(*k)?;
            sum = (sum + delegator.slashable_balance()?)?;

            Ok(())
        })?;

        sum.amount()
    }

    pub(super) fn delegator_keys(&self) -> Result<Vec<Address>> {
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

    pub(super) fn query_info(&self) -> Result<ValidatorQueryInfo> {
        Ok(ValidatorQueryInfo {
            jailed: self.jailed,
            address: self.address,
            commission: self.commission.rate,
            in_active_set: self.in_active_set,
            info: self.info.clone(),
            amount_staked: self.delegators.balance()?.amount()?,
        })
    }

    fn current_seconds(&mut self) -> Result<i64> {
        let time = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?;

        Ok(time.seconds)
    }

    pub(super) fn update_self_delegation(&mut self) -> Result<()> {
        let self_delegator = self.get_mut(self.address)?;
        let self_delegation = self_delegator.staked.amount()?;
        drop(self_delegator);
        self.self_delegation = self_delegation;

        Ok(())
    }

    fn below_required_self_delegation(&self) -> bool {
        self.self_delegation < self.min_self_delegation
    }
}

impl<S: Symbol> Balance<S, Decimal> for Validator<S> {
    fn balance(&self) -> Result<Decimal> {
        if self.jailed || !self.in_active_set || self.below_required_self_delegation() {
            Ok(0.into())
        } else {
            self.delegators.balance()
        }
    }
}

impl<S: Symbol> Give<S> for Validator<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        let one: Decimal = 1.into();
        let delegator_amount = (coins.amount * (one - self.commission.rate))?.amount()?;
        let validator_amount = (coins.amount * self.commission.rate)?.amount()?;

        self.delegators.give(delegator_amount.into())?;
        self.delegators
            .get_mut(self.address)?
            .give(validator_amount.into())?;

        Ok(())
    }
}
