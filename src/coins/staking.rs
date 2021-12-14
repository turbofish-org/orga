use super::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use super::{Address, Adjust, Amount, Balance, Coin, Decimal, Give, Pool, Share, Symbol, Take};
use crate::abci::BeginBlock;
use crate::call::Call;
use crate::client::Client;
use crate::collections::{Deque, Map};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::{BeginBlockCtx, Paid, Signer, Time, Validators};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::convert::TryInto;

#[cfg(test)]
const UNBONDING_SECONDS: u64 = 10; // 10 seconds
#[cfg(not(test))]
const UNBONDING_SECONDS: u64 = 60 * 60 * 24 * 7 * 2; // 2 weeks
const MAX_OFFLINE_BLOCKS: u64 = 1800;

type Delegators<S> = Pool<Address, Delegator<S>, S>;

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

            for hash in offline_validator_hashes {
                let val_addresses = self.val_address_for_consensus_key_hash(hash)?;
                for address in val_addresses {
                    if self.slashable_balance(address)? > 0 {
                        self.slash(address, 0)?.burn();
                    }
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
        delegator.give(coins)?;
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
        coins: Coin<S>,
    ) -> Result<()> {
        let declared = self.consensus_keys.contains_key(val_address)?;
        if declared {
            return Err(Error::Coins("Validator is already declared".into()));
        }
        self.consensus_keys
            .insert(val_address, consensus_key.into())?;

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
            self.validators.total = (self.validators.total - reduction)?;
            self.amount_delegated = (self.amount_delegated - reduction)?;
        }
        let amount = amount.into();
        let mut validator = self.get_mut(val_address)?;
        let slashed_coins = validator.slash(amount)?;
        drop(validator);

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(consensus_key, 0);

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

        delegator.liquid.take(amount)
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
        {
            let mut delegator = validator.get_mut(delegator_address)?;
            delegator.unbond(amount)?;
        }

        if !validator.jailed {
            self.amount_delegated = (self.amount_delegated - amount)?;
            validator.amount_staked = (validator.amount_staked - amount)?;
        }

        let vp = validator.staked().into();
        drop(validator);

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(consensus_key, vp);

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
    pub fn declare_self(&mut self, consensus_key: Address, amount: Amount) -> Result<()> {
        let signer = self.signer()?;
        let payment = self.paid()?.take(amount)?;
        self.declare(signer, consensus_key, payment)
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

#[derive(State)]
pub struct Validator<S: Symbol> {
    jailed: bool,
    delegators: Delegators<S>,
    jailed_coins: Amount,
    amount_staked: Amount,
}

impl<S: Symbol> Validator<S> {
    fn get_mut(&mut self, address: Address) -> Result<PoolChildMut<Address, Delegator<S>, S>> {
        self.delegators.get_mut(address)
    }

    pub fn get(&self, address: Address) -> Result<PoolChild<Delegator<S>, S>> {
        self.delegators.get(address)
    }

    pub fn staked(&self) -> Amount {
        self.amount_staked
    }

    fn slash(&mut self, amount: Amount) -> Result<Coin<S>> {
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

    fn delegator_keys(&self) -> Result<Vec<Address>> {
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

impl<S: Symbol> Adjust for Validator<S> {
    fn adjust(&mut self, multiplier: Decimal) -> Result<()> {
        self.delegators.adjust(multiplier)
    }
}

impl<S: Symbol> Balance<S, Decimal> for Validator<S> {
    fn balance(&self) -> Result<Decimal> {
        if self.jailed {
            Ok(0.into())
        } else {
            Ok(self.amount_staked.into())
        }
    }
}

impl<S: Symbol> Give<S> for Validator<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        self.delegators.give(coins)
    }
}

#[derive(State)]
pub struct Unbond<S: Symbol> {
    coins: Share<S>,
    start_seconds: i64,
}
#[derive(State)]
pub struct Delegator<S: Symbol> {
    liquid: Share<S>,
    staked: Share<S>,
    jailed: bool,
    unbonding: Deque<Unbond<S>>,
    multiplier: Decimal,
}

impl<S: Symbol> Delegator<S> {
    fn unbond<A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        let amount = amount.into();
        let coins = self.staked.take(amount)?.into();
        let start_seconds = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?
            .seconds;
        let unbond = Unbond {
            coins,
            start_seconds,
        };
        self.unbonding.push_back(unbond.into())?;

        Ok(())
    }

    fn info(&self) -> Result<DelegationInfo> {
        Ok(DelegationInfo {
            liquid: self.liquid.shares.amount()?,
            staked: self.staked.shares.amount()?,
        })
    }

    fn slashable_balance(&mut self) -> Result<Decimal> {
        self.process_unbonds()?;
        let mut sum: Decimal = 0.into();
        sum = (sum + self.staked.shares)?;
        for i in 0..self.unbonding.len() {
            let unbond = self
                .unbonding
                .get(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;
            sum = (sum + unbond.coins.shares)?;
        }

        Ok(sum)
    }

    fn slash(&mut self, multiplier: Decimal) -> Result<()> {
        self.staked.shares = (self.staked.shares * multiplier)?;
        for i in 0..self.unbonding.len() {
            let mut unbond = self
                .unbonding
                .get_mut(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;
            unbond.coins.shares = (unbond.coins.shares * multiplier)?;
        }
        self.jailed = true;
        Ok(())
    }

    fn process_unbonds(&mut self) -> Result<()> {
        let now_seconds = self
            .context::<Time>()
            .ok_or_else(|| Error::Coins("No Time context available".into()))?
            .seconds;
        while let Some(unbond) = self.unbonding.front()? {
            let unbond_matured = now_seconds - unbond.start_seconds >= UNBONDING_SECONDS as i64;
            if unbond_matured {
                let unbond = self
                    .unbonding
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Failed to pop unbond".into()))?;
                self.liquid.add(unbond.coins.shares.amount()?)?;
            } else {
                break;
            }
        }

        Ok(())
    }

    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        self.staked.give(coins)
    }
}

/// If a delegator is adjusted upward (ie. multiplier greater than one), the
/// validator has earned some reward. The reward is paid out to the delegator's
/// liquid balance.
///
/// Adjusting a jailed delegator is a no-op.
impl<S: Symbol> Adjust for Delegator<S> {
    fn adjust(&mut self, multiplier: Decimal) -> Result<()> {
        use std::cmp::Ordering::*;
        if self.multiplier == 0 {
            self.multiplier = multiplier;
        } else {
            self.multiplier = (self.multiplier * multiplier)?;
        }
        if self.jailed {
            return Ok(());
        }

        let one = 1.into();

        match multiplier.cmp(&one) {
            Greater => {
                let reward = (self.staked.shares * self.multiplier)
                    - (self.staked.shares * (self.multiplier / multiplier))?;
                self.liquid.shares = (self.liquid.shares + reward)?;
            }
            Less => {
                println!("WARNING: Downward adjustment");
            }
            Equal => (),
        }

        Ok(())
    }
}

/// A delegator's balance is its staked coins, since Balance here is used in the
/// parent collection to calculate the validator's voting power.
impl<S: Symbol> Balance<S, Decimal> for Delegator<S> {
    fn balance(&self) -> Result<Decimal> {
        if self.jailed {
            Ok(0.into())
        } else {
            Ok(self.staked.shares)
        }
    }
}

impl<S: Symbol> Balance<S, Amount> for Delegator<S> {
    fn balance(&self) -> Result<Amount> {
        self.staked.amount()
    }
}

#[derive(Encode, Decode)]
pub struct DelegationInfo {
    pub staked: Amount,
    pub liquid: Amount,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::Context,
        store::{MapStore, Shared, Store},
    };

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
        staking
            .delegate(alice, alice, Coin::mint(100))
            .expect_err("Should not be able to delegate to an undeclared validator");
        staking.declare(alice, alice_con, 50.into())?;
        staking
            .declare(alice, alice_con, 50.into())
            .expect_err("Should not be able to redeclare validator");
        staking.delegate(alice, alice, Coin::mint(50))?;
        staking.declare(bob, bob_con, 50.into())?;

        staking.delegate(bob, bob, Coin::mint(250))?;
        staking.delegate(bob, carol, Coin::mint(100))?;
        staking.delegate(bob, carol, Coin::mint(200))?;
        staking.delegate(bob, dave, Coin::mint(400))?;

        let ctx = Context::resolve::<Validators>().unwrap();

        let total_staked = staking.staked();
        assert_eq!(total_staked, 1100);

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

        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 100);

        let total_staked = staking.staked();
        assert_eq!(total_staked, 1100);

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
        let total_staked = staking.staked();
        assert_eq!(total_staked, 100);

        // Carol can still withdraw her 300 coins from Bob's jailed validator
        {
            staking.unbond(bob, carol, 150)?;
            staking
                .withdraw(bob, carol, 450)
                .expect_err("Should not be able to take coins before unbonding period has elapsed");
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

        let total_staked = staking.staked();
        assert_eq!(total_staked, 100);
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
        assert_eq!(alice_liquid, 100);
        let alice_staked = staking.get(alice)?.get(alice)?.staked.amount()?;
        assert_eq!(alice_staked, 100);

        // More block reward, but bob's delegators are jailed and should not
        // earn from it
        staking.give(Coin::mint(200))?;
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

        staking.declare(dave, dave_con, 300.into())?;
        assert_eq!(ctx.updates.get(&alice_con.bytes).unwrap().power, 100);
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 300);
        staking.delegate(dave, carol, 300.into())?;
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 600);
        staking.unbond(dave, dave, 150)?;
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 450);

        // Anonymous other validator declares so we can try jailing dave
        staking.declare([200; 32].into(), [201; 32].into(), 300.into())?;
        staking.slash(dave, 0)?.burn();
        assert_eq!(ctx.updates.get(&dave_con.bytes).unwrap().power, 0);
        staking.slash(dave, 0)?.burn();

        Ok(())
    }
}
