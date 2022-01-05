use super::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use super::{Address, Amount, Coin, Decimal, Give, Pool, Symbol};
use crate::abci::BeginBlock;
use crate::call::Call;
use crate::client::Client;
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::{BeginBlockCtx, Paid, Signer, Validators};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::convert::TryInto;

mod delegator;
pub use delegator::*;

mod validator;
pub use validator::*;

#[cfg(test)]
const UNBONDING_SECONDS: u64 = 10; // 10 seconds
#[cfg(not(test))]
const UNBONDING_SECONDS: u64 = 60 * 60 * 24 * 7 * 2; // 2 weeks
const MAX_OFFLINE_BLOCKS: u64 = 100;

#[derive(Call, Query, Client)]
pub struct Staking<S: Symbol> {
    validators: Pool<Address, Validator<S>, S>,
    amount_delegated: Amount,
    consensus_keys: Map<Address, Address>,
    last_signed_block: Map<[u8; 20], u64>,
}

impl<S: Symbol> State for Staking<S> {
    type Encoding = StakingEncoding<S>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            validators: State::create(store.sub(&[0]), data.validators)?,
            amount_delegated: State::create(store.sub(&[1]), data.amount_delegated)?,
            consensus_keys: State::create(store.sub(&[2]), ())?,
            last_signed_block: State::create(store.sub(&[3]), ())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.consensus_keys.flush()?;
        self.last_signed_block.flush()?;
        Ok(Self::Encoding {
            validators: self.validators.flush()?,
            amount_delegated: self.amount_delegated.flush()?,
        })
    }
}

impl<S: Symbol> From<Staking<S>> for StakingEncoding<S> {
    fn from(staking: Staking<S>) -> Self {
        Self {
            validators: staking.validators.into(),
            amount_delegated: staking.amount_delegated.into(),
        }
    }
}

impl<S: Symbol> BeginBlock for Staking<S> {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        if let Some(last_commit_info) = &ctx.last_commit_info {
            let height = ctx.height;
            // Update last online height
            last_commit_info
                .votes
                .iter()
                .filter(|vote_info| vote_info.signed_last_block)
                .filter(|vote_info| vote_info.validator.is_some())
                .map(|vote_info| vote_info.validator.as_ref().unwrap())
                .try_for_each(|validator| {
                    self.last_signed_block.insert(
                        validator.address[..]
                            .try_into()
                            .expect("Invalid pub key hash length"),
                        height,
                    )
                })?;

            let mut offline_validator_hashes: Vec<Vec<u8>> = vec![];
            self.last_signed_block
                .iter()?
                .try_for_each(|res| -> Result<()> {
                    let (hash, last_height) = res?;
                    if *last_height + MAX_OFFLINE_BLOCKS < height {
                        offline_validator_hashes.push(hash.to_vec());
                    }

                    Ok(())
                })?;

            for hash in offline_validator_hashes.iter() {
                let val_addresses = self.val_address_for_consensus_key_hash(hash.clone())?;
                for address in val_addresses {
                    if self.slashable_balance(address)? > 0 {
                        self.slash(address, 0)?.burn();
                    }
                    let key: [u8; 20] = hash
                        .clone()
                        .try_into()
                        .map_err(|_e| Error::Coins("Invalid pubkey hash length".into()))?;
                    self.last_signed_block.remove(key)?;
                }
            }
        }

        for evidence in &ctx.byzantine_validators {
            match &evidence.validator {
                Some(validator) => {
                    let val_addresses =
                        self.val_address_for_consensus_key_hash(validator.address.clone())?;
                    for address in val_addresses {
                        if self.slashable_balance(address)? > 0 {
                            self.slash(address, 0)?.burn();
                        }
                    }
                }
                None => {}
            }
        }

        Ok(())
    }
}

#[derive(Encode, Decode)]
pub struct StakingEncoding<S: Symbol> {
    validators: <Pool<Address, Validator<S>, S> as State>::Encoding,
    amount_delegated: <Amount as State>::Encoding,
}

impl<S: Symbol> Default for StakingEncoding<S> {
    fn default() -> Self {
        Self {
            validators: Default::default(),
            amount_delegated: Default::default(),
        }
    }
}

impl<S: Symbol> Staking<S> {
    pub fn delegate(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        coins: Coin<S>,
    ) -> Result<()> {
        let consensus_key = self.consensus_key(val_address)?;
        let mut validator = self.validators.get_mut(val_address)?;
        if validator.jailed {
            return Err(Error::Coins("Cannot delegate to jailed validator".into()));
        }
        validator.amount_staked = (validator.amount_staked + coins.amount)?;
        let mut delegator = validator.get_mut(delegator_address)?;
        self.amount_delegated = (self.amount_delegated + coins.amount)?;
        delegator.add_stake(coins)?;
        drop(delegator);
        let voting_power = validator.staked().into();
        drop(validator);

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(consensus_key, voting_power);

        Ok(())
    }

    fn consensus_key(&self, val_address: Address) -> Result<Address> {
        let consensus_key = match self.consensus_keys.get(val_address)? {
            Some(key) => *key,
            None => return Err(Error::Coins("Validator is not declared".into())),
        };

        Ok(consensus_key)
    }

    pub fn declare(
        &mut self,
        val_address: Address,
        consensus_key: Address,
        commission: Decimal,
        validator_info: ValidatorInfo,
        coins: Coin<S>,
    ) -> Result<()> {
        let declared = self.consensus_keys.contains_key(val_address)?;
        if declared {
            return Err(Error::Coins("Validator is already declared".into()));
        }
        self.consensus_keys
            .insert(val_address, consensus_key.into())?;

        let mut validator = self.validators.get_mut(val_address)?;
        validator.commission = commission;
        validator.info = validator_info;
        validator.address = val_address;
        drop(validator);

        self.delegate(val_address, val_address, coins)?;

        Ok(())
    }

    pub fn staked(&self) -> Amount {
        self.amount_delegated
    }

    pub fn slash<A: Into<Amount>>(&mut self, val_address: Address, amount: A) -> Result<Coin<S>> {
        let consensus_key = self.consensus_key(val_address)?;
        let jailed = self.get_mut(val_address)?.jailed;
        if !jailed {
            let reduction = self.slashable_balance(val_address)?;
            self.amount_delegated = (self.amount_delegated - reduction)?;
        }
        let amount = amount.into();
        let mut validator = self.get_mut(val_address)?;
        let slashed_coins = validator.slash(amount)?;
        drop(validator);

        if !jailed {
            self.context::<Validators>()
                .ok_or_else(|| Error::Coins("No Validators context available".into()))?
                .set_voting_power(consensus_key, 0);
        }

        Ok(slashed_coins)
    }

    pub fn slashable_balance(&mut self, val_address: Address) -> Result<Amount> {
        let mut validator = self.validators.get_mut(val_address)?;
        let mut sum: Decimal = 0.into();
        let delegator_keys = validator.delegator_keys()?;
        delegator_keys.iter().try_for_each(|k| -> Result<_> {
            let mut delegator = validator.get_mut(*k)?;
            sum = (sum + delegator.slashable_balance()?)?;

            Ok(())
        })?;

        sum.amount()
    }

    pub fn withdraw<A: Into<Amount>>(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        amount: A,
    ) -> Result<Coin<S>> {
        let amount = amount.into();
        let mut validator = self.validators.get_mut(val_address)?;
        let mut delegator = validator.get_mut(delegator_address)?;
        delegator.process_unbonds()?;

        delegator.withdraw_liquid(amount)
    }

    pub fn unbond<A: Into<Amount>>(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        amount: A,
    ) -> Result<()> {
        let consensus_key = self.consensus_key(val_address)?;
        let amount = amount.into();
        let mut validator = self.validators.get_mut(val_address)?;
        let vp_before = validator.staked();
        let jailed = validator.jailed;
        {
            let mut delegator = validator.get_mut(delegator_address)?;
            delegator.unbond(amount)?;
        }

        if !jailed {
            self.amount_delegated = (self.amount_delegated - amount)?;
            validator.amount_staked = (validator.amount_staked - amount)?;
        }

        let vp = validator.staked().into();
        drop(validator);

        if vp_before > 0 && !jailed {
            self.context::<Validators>()
                .ok_or_else(|| Error::Coins("No Validators context available".into()))?
                .set_voting_power(consensus_key, vp);
        }

        Ok(())
    }

    pub fn get(&self, val_address: Address) -> Result<PoolChild<Validator<S>, S>> {
        self.validators.get(val_address)
    }

    fn get_mut(&mut self, val_address: Address) -> Result<PoolChildMut<Address, Validator<S>, S>> {
        self.validators.get_mut(val_address)
    }

    #[query]
    pub fn delegations(
        &self,
        delegator_address: Address,
    ) -> Result<Vec<(Address, DelegationInfo)>> {
        self.validators
            .iter()?
            .map(|entry| {
                let (val_address, validator) = entry?;

                let delegator = validator.get(delegator_address)?;

                Ok((*val_address, delegator.info()?))
            })
            .collect()
    }

    #[call]
    pub fn unbond_self(&mut self, val_address: Address, amount: Amount) -> Result<()> {
        let signer = self.signer()?;
        self.unbond(val_address, signer, amount)
    }

    #[call]
    pub fn declare_self(
        &mut self,
        consensus_key: Address,
        commission: Decimal,
        amount: Amount,
        validator_info: ValidatorInfo,
    ) -> Result<()> {
        let signer = self.signer()?;
        let payment = self.paid()?.take(amount)?;
        self.declare(signer, consensus_key, commission, validator_info, payment)
    }

    #[call]
    pub fn delegate_from_self(&mut self, validator_address: Address, amount: Amount) -> Result<()> {
        let signer = self.signer()?;
        let payment = self.paid()?.take(amount)?;
        self.delegate(validator_address, signer, payment)
    }

    #[call]
    pub fn take_as_funding(&mut self, validator_address: Address, amount: Amount) -> Result<()> {
        let signer = self.signer()?;
        let taken_coins = self.withdraw(validator_address, signer, amount)?;
        self.paid()?.give::<S, _>(taken_coins.amount)
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Coins("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Call must be signed".into()))
    }

    fn paid(&mut self) -> Result<&mut Paid> {
        self.context::<Paid>()
            .ok_or_else(|| Error::Coins("No Payment context available".into()))
    }

    fn val_address_for_consensus_key_hash(
        &self,
        consensus_key_hash: Vec<u8>,
    ) -> Result<Vec<Address>> {
        let mut consensus_keys: Vec<(Address, Address)> = vec![];
        self.consensus_keys
            .iter()?
            .try_for_each(|entry| -> Result<()> {
                let (k, v) = entry?;
                consensus_keys.push((*k, *v));

                Ok(())
            })?;

        let val_addresses = consensus_keys
            .into_iter()
            .filter_map(|(k, v)| {
                let mut hasher = Sha256::new();
                hasher.update(v.bytes);
                let hash = hasher.finalize().to_vec();
                if hash[..20] == consensus_key_hash[..20] {
                    Some(k)
                } else {
                    None
                }
            })
            .collect();

        Ok(val_addresses)
    }
}

impl<S: Symbol> Give<S> for Staking<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        self.validators.give(coins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::Context,
        plugins::Time,
        store::{MapStore, Shared, Store},
    };
    use rust_decimal_macros::dec;

    #[derive(State, Debug, Clone)]
    struct Simp(());
    impl Symbol for Simp {}

    #[test]
    fn staking() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;

        let alice = [0; 32].into();
        let alice_con = [4; 32].into();
        let bob = [1; 32].into();
        let bob_con = [5; 32].into();
        let carol = [2; 32].into();
        let dave = [3; 32].into();
        let dave_con = [6; 32].into();

        Context::add(Validators::default());
        Context::add(Time::from_seconds(0));

        staking
            .give(100.into())
            .expect_err("Cannot give to empty validator set");
        assert_eq!(staking.staked(), 0);
        staking
            .delegate(alice, alice, Coin::mint(100))
            .expect_err("Should not be able to delegate to an undeclared validator");
        staking.declare(alice, alice_con, dec!(0.0).into(), vec![].into(), 50.into())?;
        staking
            .declare(alice, alice_con, dec!(0.0).into(), vec![].into(), 50.into())
            .expect_err("Should not be able to redeclare validator");

        assert_eq!(staking.staked(), 50);
        staking.delegate(alice, alice, Coin::mint(50))?;
        assert_eq!(staking.staked(), 100);
        staking.declare(bob, bob_con, dec!(0.0).into(), vec![].into(), 50.into())?;
        assert_eq!(staking.staked(), 150);

        staking.delegate(bob, bob, Coin::mint(250))?;
        staking.delegate(bob, carol, Coin::mint(100))?;
        staking.delegate(bob, carol, Coin::mint(200))?;
        staking.delegate(bob, dave, Coin::mint(400))?;
        assert_eq!(staking.staked(), 1100);

        let ctx = Context::resolve::<Validators>().unwrap();

        let alice_vp = ctx.updates.get(&alice_con.bytes).unwrap().power;
        assert_eq!(alice_vp, 100);

        let bob_vp = ctx.updates.get(&bob_con.bytes).unwrap().power;
        assert_eq!(bob_vp, 1000);

        let alice_self_delegation = staking.get(alice)?.get(alice)?.staked.amount()?;
        assert_eq!(alice_self_delegation, 100);

        let bob_self_delegation = staking.get(bob)?.get(bob)?.staked.amount()?;
        assert_eq!(bob_self_delegation, 300);

        let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
        assert_eq!(carol_to_bob_delegation, 300);

        let alice_val_balance = staking.get(alice)?.staked();
        assert_eq!(alice_val_balance, 100);

        let bob_val_balance = staking.get(bob)?.staked();
        assert_eq!(bob_val_balance, 1000);

        // Big block rewards, doubling all balances
        staking.give(Coin::mint(600))?;
        staking.give(Coin::mint(500))?;
        assert_eq!(staking.staked(), 1100);

        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 100);

        let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
        assert_eq!(carol_to_bob_delegation, 300);
        let carol_to_bob_liquid = staking.get(bob)?.get(carol)?.liquid.amount()?;
        assert_eq!(carol_to_bob_liquid, 300);

        let bob_val_balance = staking.get(bob)?.staked();
        assert_eq!(bob_val_balance, 1000);

        let bob_vp = ctx.updates.get(&bob_con.bytes).unwrap().power;
        assert_eq!(bob_vp, 1000);

        // Bob gets slashed 50%
        let slashed_coins = staking.slash(bob, 500)?;
        assert_eq!(slashed_coins.amount, 500);
        slashed_coins.burn();

        // Make sure it's now impossible to delegate to Bob
        staking
            .delegate(bob, alice, 200.into())
            .expect_err("Should not be able to delegate to jailed validator");
        staking
            .delegate(bob, bob, 200.into())
            .expect_err("Should not be able to delegate to jailed validator");

        // Bob has been jailed and should no longer have any voting power
        let bob_vp = ctx.updates.get(&bob_con.bytes).unwrap().power;
        assert_eq!(bob_vp, 0);

        // Bob's staked coins should no longer be present in the global staking
        // balance
        assert_eq!(staking.staked(), 100);

        // Carol can still withdraw her 300 coins from Bob's jailed validator
        {
            staking.unbond(bob, carol, 150)?;
            assert_eq!(staking.staked(), 100);
            staking
                .withdraw(bob, carol, 450)
                .expect_err("Should not be able to take coins before unbonding period has elapsed");
            assert_eq!(staking.staked(), 100);
            Context::add(Time::from_seconds(10));
            let carol_recovered_coins = staking.withdraw(bob, carol, 450)?;

            assert_eq!(carol_recovered_coins.amount, 450);
        }

        {
            // Bob withdraws a third of his self-delegation
            staking.unbond(bob, bob, 100)?;
            Context::add(Time::from_seconds(20));
            let bob_recovered_coins = staking.withdraw(bob, bob, 100)?;
            assert_eq!(bob_recovered_coins.amount, 100);
            staking
                .unbond(bob, bob, 201)
                .expect_err("Should not be able to unbond more than we have staked");

            staking.unbond(bob, bob, 50)?;
            Context::add(Time::from_seconds(30));
            staking
                .withdraw(bob, bob, 501)
                .expect_err("Should not be able to take more than we have unbonded");
            staking.withdraw(bob, bob, 350)?.burn();
        }

        assert_eq!(staking.staked(), 100);
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 100);
        let alice_staked = staking.get(alice)?.get(alice)?.staked.amount()?;
        assert_eq!(alice_staked, 100);

        // More block reward, but bob's delegators are jailed and should not
        // earn from it
        staking.give(Coin::mint(200))?;
        assert_eq!(staking.staked(), 100);
        let alice_val_balance = staking.get(alice)?.staked();
        assert_eq!(alice_val_balance, 100);
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 300);

        staking
            .unbond(bob, dave, 401)
            .expect_err("Dave should only have 400 unbondable coins");

        assert_eq!(staking.slashable_balance(bob)?, 200);
        staking.unbond(bob, dave, 200)?;
        // Bob slashed another 50% while Dave unbonds
        assert_eq!(staking.slashable_balance(bob)?, 200);
        staking.slash(bob, 100)?.burn();
        assert_eq!(staking.slashable_balance(bob)?, 100);
        staking
            .withdraw(bob, dave, 401)
            .expect_err("Dave cannot take coins yet");
        Context::add(Time::from_seconds(40));
        staking
            .withdraw(bob, dave, 501)
            .expect_err("Dave cannot take so many coins");
        assert_eq!(staking.slashable_balance(bob)?, 0);
        staking.withdraw(bob, dave, 500)?.burn();
        assert_eq!(staking.slashable_balance(bob)?, 0);

        assert_eq!(staking.staked(), 100);
        staking.declare(dave, dave_con, dec!(0.0).into(), vec![].into(), 300.into())?;
        assert_eq!(staking.staked(), 400);
        assert_eq!(ctx.updates.get(&alice_con.bytes).unwrap().power, 100);
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 300);
        staking.delegate(dave, carol, 300.into())?;
        assert_eq!(staking.staked(), 700);
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 600);
        staking.unbond(dave, dave, 150)?;
        assert_eq!(staking.staked(), 550);
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 450);

        // Test commissions
        let edith = [7; 32].into();
        let edith_con = [201; 32].into();

        dbg!(staking.staked());

        staking.declare(
            edith,
            edith_con,
            dec!(0.5).into(),
            vec![].into(),
            550.into(),
        )?;

        staking.delegate(edith, carol, 550.into())?;

        staking.get_mut(edith)?.give(500.into())?;

        let edith_liquid = staking.get(edith)?.get(edith)?.liquid.amount()?;
        assert_eq!(edith_liquid, 375);
        let carol_liquid = staking.get(edith)?.get(carol)?.liquid.amount()?;
        assert_eq!(carol_liquid, 125);

        staking.slash(dave, 0)?.burn();
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 0);
        staking.slash(dave, 0)?.burn();

        Ok(())
    }
}
