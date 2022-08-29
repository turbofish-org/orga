use super::{Commission, Declaration, Staking};
use crate::coins::{Address, Decimal, Give, Symbol};
use crate::encoding::Decode;
use crate::migrate::Migrate;
use crate::plugins::EndBlockCtx;
use crate::Result;
use v3::encoding::Encode as EncodeV3;

impl From<v3::coins::Decimal> for Decimal {
    fn from(decimal: v3::coins::Decimal) -> Self {
        Decode::decode(decimal.encode().unwrap().as_slice()).unwrap()
    }
}

fn liquid_balances<S: v3::coins::Symbol>(delegator: &v3::coins::Delegator<S>) -> Vec<(u8, u64)> {
    let mut unbonding_sum: u64 = 0;
    for i in 0..delegator.unbonding.len() {
        let unbond = delegator.unbonding.get(i).unwrap().unwrap();
        let unbond_amt: u64 = unbond.coins.amount().unwrap().into();
        unbonding_sum += unbond_amt;
    }
    let liquid: Vec<(u8, u64)> = delegator
        .liquid
        .amounts()
        .unwrap()
        .into_iter()
        .map(|(symbol, amt)| -> (u8, u64) { (symbol, amt.into()) })
        .map(|(symbol, amt)| {
            if symbol == S::INDEX {
                (symbol, unbonding_sum + amt)
            } else {
                (symbol, amt)
            }
        })
        .collect();

    liquid
}

impl<S: Symbol> Staking<S> {
    fn migrate_validators<T: v3::coins::Symbol>(
        &mut self,
        legacy: &v3::coins::Staking<T>,
    ) -> Result<()> {
        legacy.validators().iter().unwrap().try_for_each(|entry| {
            let (address, validator) = entry.unwrap();
            let consensus_key = legacy.consensus_keys().get(address).unwrap().unwrap();
            self.migrate_validator(address.bytes().into(), &validator, *consensus_key)
        })
    }

    fn migrate_validator<T: v3::coins::Symbol>(
        &mut self,
        val_addr: Address,
        validator: &v3::coins::pool::Child<v3::coins::staking::Validator<T>, T>,
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
            for (symbol, amt) in liquid_balances(&legacy_delegator) {
                delegator.give((symbol, amt.into()))?;
            }
            Ok::<(), crate::Error>(())
        })?;

        self.update_vp(val_addr)
    }
}

impl<S: Symbol, T: v3::coins::Symbol> Migrate<v3::coins::Staking<T>> for super::Staking<S> {
    fn migrate(&mut self, legacy: v3::coins::Staking<T>) -> Result<()> {
        self.migrate_validators(&legacy)?;
        self.end_block_step(&EndBlockCtx { height: 0 })
    }
}
