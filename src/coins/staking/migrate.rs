use super::{Commission, Declaration, Staking};
use crate::coins::{Address, Amount, Decimal, Give, Symbol};
use crate::encoding::Decode;
use crate::migrate::Migrate;
use crate::plugins::EndBlockCtx;
use crate::Result;
use rust_decimal_macros::dec;
use v1::encoding::Encode as EncodeV1;

impl From<v1::coins::Decimal> for Decimal {
    fn from(decimal: v1::coins::Decimal) -> Self {
        Decode::decode(decimal.encode().unwrap().as_slice()).unwrap()
    }
}

fn liquid_balance<S: v1::coins::Symbol>(delegator: &v1::coins::Delegator<S>) -> Amount {
    let liquid: u64 = delegator.liquid.amount().unwrap().into();

    if liquid < 10_000 && delegator.jailed {
        return liquid.into();
    }

    let mut unbonding_sum: u64 = 0;

    for i in 0..delegator.unbonding.len() {
        let unbond = delegator.unbonding.get(i).unwrap().unwrap();
        let unbond_amt: u64 = unbond.coins.amount().unwrap().into();
        unbonding_sum += unbond_amt;
    }

    (liquid + unbonding_sum).into()
}

impl<S: Symbol> Staking<S> {
    fn migrate_validators<T: v1::coins::Symbol>(
        &mut self,
        legacy: &v1::coins::Staking<T>,
    ) -> Result<()> {
        legacy.validators().iter().unwrap().try_for_each(|entry| {
            let (address, validator) = entry.unwrap();
            let consensus_key = legacy.consensus_keys().get(address).unwrap().unwrap();
            self.migrate_validator(address.bytes().into(), &validator, *consensus_key)
        })
    }

    fn migrate_validator<T: v1::coins::Symbol>(
        &mut self,
        val_addr: Address,
        validator: &v1::coins::pool::Child<v1::coins::staking::Validator<T>, T>,
        consensus_key: [u8; 32],
    ) -> Result<()> {
        let declaration = Declaration {
            consensus_key,
            commission: Commission {
                rate: validator.commission.into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.01).into(),
            },
            min_self_delegation: self.min_self_delegation_min.into(),
            amount: 0.into(),
            validator_info: validator.info.bytes.clone().into(),
        };

        let self_del = validator.delegators.get(val_addr.bytes().into()).unwrap();
        let amt: u64 = self_del.staked.amount().unwrap().into();
        self.declare(val_addr, declaration, amt.into())?;

        validator.delegators.iter().unwrap().try_for_each(|entry| {
            let (del_addr, legacy_delegator) = entry.unwrap();
            if del_addr.bytes() != val_addr.bytes() {
                let staked_amt: u64 = legacy_delegator.staked.amount().unwrap().into();
                self.delegate(val_addr, del_addr.bytes().into(), staked_amt.into())?;
            }
            let mut validator = self.validators.get_mut(val_addr)?;
            let mut delegator = validator.get_mut(del_addr.bytes().into())?;
            delegator.give(liquid_balance(&legacy_delegator).into())
        })
    }
}

impl<S: Symbol, T: v1::coins::Symbol> Migrate<v1::coins::Staking<T>> for super::Staking<S> {
    fn migrate(&mut self, legacy: v1::coins::Staking<T>) -> Result<()> {
        self.migrate_validators(&legacy)?;
        self.end_block_step(&EndBlockCtx { height: 0 })
    }
}
