use super::{Commission, Declaration, Staking, Validator, ValidatorInfo};
use crate::coins::{Address, Child, Give, Symbol};
use crate::migrate::Migrate;
use crate::Result;
use rust_decimal_macros::dec;

// migration from 3e88fabcb548895fad1652f0c9f2ee96ca7de0b5
mod v1 {
    use crate::coins::{Address, Amount, Balance, Coin, Decimal, Give, Pool, Share, Symbol};
    use crate::collections::{Deque, Entry as EntryTrait, EntryMap, Map};
    use crate::encoding::{Decode, Encode, Terminated};
    use crate::state::State;
    use crate::store::Store;
    use crate::Result;

    const MAX_VALIDATORS: u64 = 100;
    const MIN_SELF_DELEGATION: u64 = 1;

    pub struct Staking<S: Symbol> {
        pub validators: Pool<Address, Validator<S>, S>,
        pub consensus_keys: Map<Address, [u8; 32]>,
        pub last_signed_block: Map<[u8; 20], u64>,
        pub max_validators: u64,
        pub min_self_delegation: u64,
        pub validators_by_power: EntryMap<ValidatorPowerEntry>,
        pub last_indexed_power: Map<Address, u64>,
        pub last_validator_powers: Map<Address, u64>,
        pub address_for_tm_hash: Map<[u8; 20], Address>,
    }

    #[derive(EntryTrait)]
    pub struct ValidatorPowerEntry {
        #[key]
        inverted_power: u64,
        #[key]
        address_bytes: [u8; 20],
    }

    impl ValidatorPowerEntry {
        pub fn power(&self) -> u64 {
            u64::max_value() - self.inverted_power
        }
    }

    impl<S: Symbol> State for Staking<S> {
        type Encoding = StakingEncoding<S>;

        fn create(store: Store, data: Self::Encoding) -> Result<Self> {
            Ok(Self {
                validators: State::create(store.sub(&[0]), data.validators)?,
                min_self_delegation: State::create(store.sub(&[1]), data.min_self_delegation)?,
                consensus_keys: State::create(store.sub(&[2]), ())?,
                last_signed_block: State::create(store.sub(&[3]), ())?,
                validators_by_power: State::create(store.sub(&[4]), ())?,
                last_validator_powers: State::create(store.sub(&[5]), ())?,
                max_validators: State::create(store.sub(&[6]), data.max_validators)?,
                last_indexed_power: State::create(store.sub(&[7]), ())?,
                address_for_tm_hash: State::create(store.sub(&[8]), ())?,
            })
        }

        fn flush(self) -> Result<Self::Encoding> {
            unimplemented!()
        }
    }

    impl<S: Symbol> From<Staking<S>> for StakingEncoding<S> {
        fn from(staking: Staking<S>) -> Self {
            Self {
                max_validators: staking.max_validators,
                min_self_delegation: staking.min_self_delegation,
                validators: staking.validators.into(),
            }
        }
    }

    #[derive(Encode, Decode)]
    pub struct StakingEncoding<S: Symbol> {
        max_validators: u64,
        min_self_delegation: u64,
        validators: <Pool<Address, Validator<S>, S> as State>::Encoding,
    }

    impl<S: Symbol> Default for StakingEncoding<S> {
        fn default() -> Self {
            Self {
                max_validators: MAX_VALIDATORS,
                min_self_delegation: MIN_SELF_DELEGATION,
                validators: Default::default(),
            }
        }
    }

    type Delegators<S> = Pool<Address, Delegator<S>, S>;

    #[derive(State)]
    pub struct Validator<S: Symbol> {
        pub jailed: bool,
        pub address: Address,
        pub commission: Decimal,
        pub delegators: Delegators<S>,
        pub info: ValidatorInfo,
        pub in_active_set: bool,
    }

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

    impl<S: Symbol> Balance<S, Decimal> for Validator<S> {
        fn balance(&self) -> Result<Decimal> {
            if self.jailed || !self.in_active_set {
                Ok(0.into())
            } else {
                self.delegators.balance()
            }
        }
    }

    impl<S: Symbol> Give<S> for Validator<S> {
        fn give(&mut self, coins: Coin<S>) -> Result<()> {
            let one: Decimal = 1.into();
            let delegator_amount = (coins.amount * (one - self.commission))?.amount()?;
            let validator_amount = (coins.amount * self.commission)?.amount()?;

            self.delegators.give(delegator_amount.into())?;
            self.delegators
                .get_mut(self.address)?
                .give(validator_amount.into())?;

            Ok(())
        }
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
            unimplemented!()
        }

        fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
            unimplemented!()
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

    #[derive(State)]
    pub struct Unbond<S: Symbol> {
        pub coins: Share<S>,
        pub start_seconds: i64,
    }

    #[derive(State)]
    pub struct Delegator<S: Symbol> {
        pub liquid: Share<S>,
        pub staked: Share<S>,
        pub jailed: bool,
        pub unbonding: Deque<Unbond<S>>,
    }

    impl<S: Symbol> Delegator<S> {
        pub fn liquid_balance(&self) -> Result<Amount> {
            let pending_rewards = self.liquid.amount()?;

            if self.liquid.amount()? < 10_000 && self.jailed {
                return Ok(pending_rewards);
            }

            let mut unbonding_sum: Amount = 0.into();

            for i in 0..self.unbonding.len() {
                let unbond = self.unbonding.get(i)?.unwrap();
                unbonding_sum = (unbonding_sum + unbond.coins.amount()?)?;
            }

            (pending_rewards + unbonding_sum).result()
        }
    }
}

impl<S: Symbol> super::Staking<S> {
    fn migrate_validators(&mut self, legacy: &v1::Staking<S>) -> Result<()> {
        legacy.validators.iter()?.try_for_each(|entry| {
            let (address, validator) = entry?;
            let consensus_key = legacy.consensus_keys.get(address)?.unwrap();
            self.migrate_validator(address, &validator, *consensus_key)
        })
    }

    fn migrate_validator(
        &mut self,
        val_addr: Address,
        validator: &Child<v1::Validator<S>, S>,
        consensus_key: [u8; 32],
    ) -> Result<()> {
        let declaration = Declaration {
            consensus_key,
            commission: Commission {
                rate: validator.commission,
                max: dec!(1.0).into(),
                max_change: dec!(0.01).into(),
            },
            min_self_delegation: self.min_self_delegation_min.into(),
            amount: 0.into(),
            validator_info: validator.info.bytes.clone().into(),
        };

        let self_del = validator.delegators.get(val_addr)?;
        self.declare(val_addr, declaration, self_del.staked.amount()?.into())?;

        validator.delegators.iter()?.try_for_each(|entry| {
            let (del_addr, legacy_delegator) = entry?;
            if del_addr != val_addr {
                self.delegate(val_addr, del_addr, legacy_delegator.staked.amount()?.into())?;
            }
            let mut validator = self.validators.get_mut(val_addr)?;
            let mut delegator = validator.get_mut(del_addr)?;
            delegator.give(legacy_delegator.liquid_balance()?.into())
        })
    }
}

impl<S: Symbol> Migrate for super::Staking<S> {
    type Legacy = v1::Staking<S>;

    fn migrate(&mut self, legacy: Self::Legacy) -> Result<()> {
        self.migrate_validators(&legacy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coins::{Accounts, Staking};
    use crate::encoding::{Decode, Encode};
    use crate::merk::MerkStore;
    use crate::state::State;
    use crate::store::{MapStore, Read, Shared, Store, Write};

    type OldEncoding = (
        <Accounts<Simp> as State>::Encoding,
        <v1::Staking<Simp> as State>::Encoding,
    );

    #[derive(State, Debug, Clone)]
    struct Simp(());
    impl Symbol for Simp {}

    #[test]
    fn load_store() -> Result<()> {
        let path = std::path::PathBuf::from(env!("PWD"))
            .join("target")
            .join("merk");
        let old_store = Store::new(Shared::new(MerkStore::new(path)).into());
        let data_bytes = old_store.get(&[])?.unwrap();
        let data = OldEncoding::decode(data_bytes.as_slice())?;

        let old_store = old_store.sub(&[0, 1, 0, 1]);
        let old_data = data.1;

        let new_store = Store::new(Shared::new(MapStore::new()).into());

        let old_staking: v1::Staking<Simp> = State::create(old_store, old_data)?;
        let mut new_staking: Staking<Simp> = State::create(new_store, Default::default())?;

        new_staking.migrate(old_staking)?;

        let addr = "nomic1ak0v68rxfdug9tdkalxks674gm2hrn0l70xytc"
            .parse()
            .unwrap();
        let delegations = new_staking.delegations(addr)?;

        println!("{:#?}", delegations);

        Ok(())
    }
}
