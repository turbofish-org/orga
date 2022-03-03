use super::{Commission, Declaration, Staking, Validator, ValidatorInfo};
use crate::coins::{Address, Child, Give, Symbol};
use crate::migrate::Migrate;
use crate::Result;
use rust_decimal_macros::dec;

impl<S: Symbol> Staking<S> {
    fn migrate_validators<T: orgav1::coins::Symbol>(
        &mut self,
        legacy: &orgav1::coins::Staking<T>,
    ) -> Result<()> {
        legacy.validators.iter()?.try_for_each(|entry| {
            let (address, validator) = entry?;
            let consensus_key = legacy.consensus_keys.get(address)?.unwrap();
            self.migrate_validator(address, &validator, *consensus_key)
        })
    }

    fn migrate_validator(
        &mut self,
        val_addr: Address,
        validator: &orgav1::coins::pool::Child<orgav1::coins::staking::Validator<T>, T>,
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
use orgav1::state::State as OldState;

#[derive(Debug, Clone)]
struct Sym(());
impl OldState for Sym {
    type Encoding = ();

    fn create(_store: orgav1::store::Store, _data: Self::Encoding) -> orgav1::Result<Self> {
        todo!()
    }

    fn flush(self) -> orgav1::Result<Self::Encoding> {
        todo!()
    }
}
impl From<Sym> for () {
    fn from(_: Sym) -> Self {}
}
impl orgav1::coins::Symbol for Sym {}

impl<S: Symbol> Migrate for super::Staking<S> {
    type Legacy = orgav1::coins::Staking<Sym>;

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
    use orgav1::encoding::{Decode as OldDecode, Encode as OldEncode};
    use orgav1::merk::MerkStore as OldMerkStore;
    use orgav1::store::{
        Read as OldRead, Shared as OldShared, Store as OldStore, Write as OldWrite,
    };

    type OldEncoding = (
        <orgav1::coins::Accounts<Sym> as orgav1::state::State>::Encoding,
        <orgav1::coins::Staking<Sym> as orgav1::state::State>::Encoding,
    );

    #[derive(State, Debug, Clone)]
    struct Simp(());
    impl Symbol for Simp {}

    #[test]
    fn load_store() -> Result<()> {
        let path = std::path::PathBuf::from(env!("PWD"))
            .join("target")
            .join("merk");
        let old_store = OldStore::new(OldShared::new(OldMerkStore::new(path)).into());
        let data_bytes = old_store.get(&[]).unwrap().unwrap();
        let data = OldEncoding::decode(data_bytes.as_slice()).unwrap();

        let old_store = old_store.sub(&[0, 1, 0, 1]);
        let old_data = data.1;

        let new_store = Store::new(Shared::new(MapStore::new()).into());

        let old_staking: orgav1::coins::Staking<Sym> =
            OldState::create(old_store, old_data).unwrap();
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
