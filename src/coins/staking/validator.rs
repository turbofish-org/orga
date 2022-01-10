use crate::coins::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use crate::coins::{Address, Amount, Balance, Coin, Decimal, Give, Pool, Symbol};
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::Result;
use ed::Terminated;

use super::Delegator;

type Delegators<S> = Pool<Address, Delegator<S>, S>;
#[derive(State)]
pub struct Validator<S: Symbol> {
    pub(super) jailed: bool,
    pub(super) address: Address,
    pub(super) commission: Decimal,
    pub(super) delegators: Delegators<S>,
    pub(super) jailed_coins: Amount,
    pub(super) amount_staked: Amount,
    pub(super) info: ValidatorInfo,
}

#[derive(Default)]
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

    pub fn staked(&self) -> Amount {
        self.amount_staked
    }

    pub(super) fn slash(&mut self, amount: Amount) -> Result<Coin<S>> {
        self.jailed = true;
        let one: Decimal = 1.into();
        let slash_multiplier = (one - (amount / self.slashable_balance()?))?;
        let delegator_keys = self.delegator_keys()?;
        delegator_keys.iter().try_for_each(|k| -> Result<()> {
            let mut delegator = self.get_mut(*k)?;
            delegator.slash(slash_multiplier)?;
            Ok(())
        })?;
        self.amount_staked = 0.into();

        Ok(amount.into())
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
                delegator_keys.push(*k);

                Ok(())
            })?;

        Ok(delegator_keys)
    }
}

impl<S: Symbol> Balance<S, Decimal> for Validator<S> {
    fn balance(&self) -> Result<Decimal> {
        if self.jailed {
            Ok(0.into())
        } else {
            // Ok(self.amount_staked.into())
            self.delegators.balance()
        }
    }
}

impl<S: Symbol> Give<S> for Validator<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        let one: Decimal = 1.into();
        let delegator_amount = (coins.amount * (one - self.commission))?.amount()?;
        let validator_amount = (coins.amount * self.commission)?.amount()?;

        debug_assert_eq!(
            (delegator_amount + validator_amount).result().unwrap(),
            coins.amount
        );

        self.delegators.give(delegator_amount.into())?;
        self.delegators
            .get_mut(self.address)?
            .give(validator_amount.into())?;

        Ok(())
    }
}
