use super::{Commission, Declaration, Staking};
use crate::coins::{Address, Amount, Decimal, Give, Symbol};
use crate::encoding::Decode;
use crate::migrate::Migrate;
use crate::plugins::EndBlockCtx;
use crate::Result;
use v2::encoding::Encode as EncodeV2;

impl From<v2::coins::Decimal> for Decimal {
    fn from(decimal: v2::coins::Decimal) -> Self {
        Decode::decode(decimal.encode().unwrap().as_slice()).unwrap()
    }
}

fn liquid_balance<S: v2::coins::Symbol>(
    delegator: &v2::coins::Delegator<S>,
    _now_seconds: i64,
) -> Amount {
    let liquid: u64 = delegator.liquid.amount().unwrap().into();

    let mut unbonding_sum: u64 = 0;

    for i in 0..delegator.unbonding.len() {
        let unbond = delegator.unbonding.get(i).unwrap().unwrap();
        let unbond_amt: u64 = unbond.coins.amount().unwrap().into();
        unbonding_sum += unbond_amt;
    }

    (liquid + unbonding_sum).into()
}

impl<S: Symbol> Staking<S> {
    fn migrate_validators<T: v2::coins::Symbol>(
        &mut self,
        legacy: &v2::coins::Staking<T>,
    ) -> Result<()> {
        legacy.validators().iter().unwrap().try_for_each(|entry| {
            let (address, validator) = entry.unwrap();
            let consensus_key = legacy.consensus_keys().get(address).unwrap().unwrap();
            self.migrate_validator(address.bytes().into(), &validator, *consensus_key)
        })
    }

    fn migrate_validator<T: v2::coins::Symbol>(
        &mut self,
        val_addr: Address,
        validator: &v2::coins::pool::Child<v2::coins::staking::Validator<T>, T>,
        consensus_key: [u8; 32],
    ) -> Result<()> {
        let declaration = Declaration {
            consensus_key,
            commission: Commission {
                max: validator.commission.max.into(),
                max_change: validator.commission.max_change.into(),
                rate: validator.commission.rate.into(),
            },
            min_self_delegation: self.min_self_delegation_min.into(),
            amount: 0.into(),
            validator_info: validator.info.bytes.clone().into(),
        };
        let now_seconds = self.current_seconds()?;

        let self_del = validator.delegators.get(val_addr.bytes().into()).unwrap();
        let amt: u64 = self_del.staked.amount().unwrap().into();
        self.declare(val_addr, declaration, amt.into())?;
        if validator.jailed() {
            let mut new_validator = self.validators.get_mut(val_addr)?;
            new_validator.jail_for_seconds(10)?;
        }

        validator.delegators.iter().unwrap().try_for_each(|entry| {
            let (del_addr, legacy_delegator) = entry.unwrap();
            if del_addr.bytes() != val_addr.bytes() {
                let staked_amt: u64 = legacy_delegator.staked.amount().unwrap().into();
                self.delegate(val_addr, del_addr.bytes().into(), staked_amt.into())?;
            }
            let mut validator = self.validators.get_mut(val_addr)?;
            let mut delegator = validator.get_mut(del_addr.bytes().into())?;
            delegator.give(liquid_balance(&legacy_delegator, now_seconds).into())
        })?;

        self.update_vp(val_addr)
    }
}

impl<S: Symbol, T: v2::coins::Symbol> Migrate<v2::coins::Staking<T>> for super::Staking<S> {
    fn migrate(&mut self, legacy: v2::coins::Staking<T>) -> Result<()> {
        self.migrate_validators(&legacy)?;
        self.end_block_step(&EndBlockCtx { height: 0 })
    }
}
