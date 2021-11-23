use super::pool::{Child as PoolChild, ChildMut as PoolChildMut};
use super::{Address, Adjust, Amount, Balance, Coin, Give, Pool, Ratio, Share, Symbol, Take};
use crate::collections::Deque;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::{Time, Validators};
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};

type Delegators<S, const U: u64> = Pool<Address, Delegator<S, U>, S>;

pub struct Staking<S: Symbol, const U: u64 = 0> {
    vp_per_coin: Ratio,
    validators: Pool<Address, Validator<S, U>, S>,
    amount_delegated: Amount,
}

impl<S: Symbol, const U: u64> State for Staking<S, U> {
    type Encoding = StakingEncoding<S, U>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            vp_per_coin: <Ratio as State>::create(store.sub(&[0]), data.vp_per_coin)?,
            validators: State::create(store.sub(&[1]), data.validators)?,
            amount_delegated: State::create(store.sub(&[2]), data.amount_delegated)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(Self::Encoding {
            vp_per_coin: <Ratio as State>::flush(self.vp_per_coin)?,
            validators: self.validators.flush()?,
            amount_delegated: self.amount_delegated.flush()?,
        })
    }
}

impl<S: Symbol, const U: u64> From<Staking<S, U>> for StakingEncoding<S, U> {
    fn from(staking: Staking<S, U>) -> Self {
        Self {
            vp_per_coin: staking.vp_per_coin.into(),
            validators: staking.validators.into(),
            amount_delegated: staking.amount_delegated.into(),
        }
    }
}

#[derive(Encode, Decode)]
pub struct StakingEncoding<S: Symbol, const U: u64> {
    vp_per_coin: <Ratio as State>::Encoding,
    validators: <Pool<Address, Validator<S, U>, S> as State>::Encoding,
    amount_delegated: <Amount as State>::Encoding,
}

impl<S: Symbol, const U: u64> Default for StakingEncoding<S, U> {
    fn default() -> Self {
        Self {
            vp_per_coin: Ratio::new(1, 1).unwrap().into(),
            validators: Default::default(),
            amount_delegated: Default::default(),
        }
    }
}

impl<S: Symbol, const U: u64> Staking<S, U> {
    pub fn delegate(
        &mut self,
        val_address: Address,
        delegator_address: Address,
        coins: Coin<S>,
    ) -> Result<()> {
        let mut validator = self.validators.get_mut(val_address)?;
        validator.amount_staked = (validator.amount_staked + coins.amount)?;
        let mut delegator = validator.get_mut(delegator_address)?;
        self.amount_delegated = (self.amount_delegated + coins.amount)?;
        delegator.give(coins)?;
        drop(delegator);
        let voting_power = (validator.balance() * self.vp_per_coin)?.to_integer();
        drop(validator);

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(val_address, voting_power);
        Ok(())
    }

    pub fn staked(&self) -> Amount {
        self.amount_delegated
    }

    pub fn slash<A: Into<Amount>>(&mut self, val_address: Address, amount: A) -> Result<Coin<S>> {
        let jailed = self.get_mut(val_address)?.jailed;
        if !jailed {
            let reduction = self.slashable_balance(val_address)?;
            self.validators.total = (self.validators.total - reduction)?;
            self.amount_delegated = (self.amount_delegated - reduction)?;
        }
        let amount = amount.into();
        let staked_before = self.staked();
        let mut validator = self.get_mut(val_address)?;
        let slashed_coins = validator.slash(amount)?;
        drop(validator);
        let staked_after = self.staked();

        self.vp_per_coin = (self.vp_per_coin * (staked_after / staked_before))?;

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(val_address, 0);

        Ok(slashed_coins)
    }

    pub fn slashable_balance(&mut self, val_address: Address) -> Result<Amount> {
        let mut validator = self.validators.get_mut(val_address)?;
        let mut sum: Ratio = 0.into();
        let delegator_keys = validator.delegator_keys()?;
        delegator_keys.iter().try_for_each(|k| -> Result<_> {
            let mut delegator = validator.get_mut(*k)?;
            sum = (sum + delegator.slashable_balance()?)?;

            Ok(())
        })?;

        Ok(sum.amount())
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

        Ok(())
    }

    pub fn get(&self, val_address: Address) -> Result<PoolChild<Validator<S, U>, S>> {
        self.validators.get(val_address)
    }

    pub fn get_mut(
        &mut self,
        val_address: Address,
    ) -> Result<PoolChildMut<Address, Validator<S, U>, S>> {
        self.validators.get_mut(val_address)
    }
}

impl<S: Symbol, const U: u64> Give<S> for Staking<S, U> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        // TODO: Handle giving to empty pool
        self.validators.give(coins)
    }
}

#[derive(State)]
pub struct Validator<S: Symbol, const U: u64> {
    jailed: bool,
    delegators: Delegators<S, U>,
    jailed_coins: Amount,
    amount_staked: Amount,
}

impl<S: Symbol, const U: u64> Validator<S, U> {
    pub fn get_mut(
        &mut self,
        address: Address,
    ) -> Result<PoolChildMut<Address, Delegator<S, U>, S>> {
        self.delegators.get_mut(address)
    }

    pub fn get(&self, address: Address) -> Result<PoolChild<Delegator<S, U>, S>> {
        self.delegators.get(address)
    }

    pub fn staked(&self) -> Amount {
        self.amount_staked
    }

    fn slash(&mut self, amount: Amount) -> Result<Coin<S>> {
        // let slashed_coins = self.take(amount)?;
        self.jailed = true;
        let one: Ratio = 1.into();
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
        let mut sum: Ratio = 0.into();
        let delegator_keys = self.delegator_keys()?;
        delegator_keys.iter().try_for_each(|k| -> Result<_> {
            let mut delegator = self.get_mut(*k)?;
            sum = (sum + delegator.slashable_balance()?)?;

            Ok(())
        })?;

        Ok(sum.amount())
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

impl<S: Symbol, const U: u64> Adjust for Validator<S, U> {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        self.delegators.adjust(multiplier)
    }
}

impl<S: Symbol, const U: u64> Balance<S, Ratio> for Validator<S, U> {
    fn balance(&self) -> Ratio {
        if self.jailed {
            0.into()
        } else {
            self.amount_staked.into()
        }
    }
}

impl<S: Symbol, const U: u64> Give<S> for Validator<S, U> {
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
pub struct Delegator<S: Symbol, const U: u64> {
    liquid: Share<S>,
    staked: Share<S>,
    jailed: bool,
    unbonding: Deque<Unbond<S>>,
    multiplier: Ratio,
}

impl<S: Symbol, const U: u64> Delegator<S, U> {
    pub fn unbond<A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
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

    fn slashable_balance(&mut self) -> Result<Ratio> {
        self.process_unbonds()?;
        let mut sum: Ratio = 0.into();
        sum = (sum + self.staked.amount)?;
        for i in 0..self.unbonding.len() {
            let unbond = self
                .unbonding
                .get(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;
            sum = (sum + unbond.coins.amount)?;
        }

        Ok(sum)
    }

    fn slash(&mut self, multiplier: Ratio) -> Result<()> {
        self.staked.amount = (self.staked.amount * multiplier)?;
        for i in 0..self.unbonding.len() {
            let mut unbond = self
                .unbonding
                .get_mut(i)?
                .ok_or_else(|| Error::Coins("Failed to iterate over unbonds".into()))?;
            unbond.coins.amount = (unbond.coins.amount * multiplier)?;
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
            let unbond_matured = now_seconds - unbond.start_seconds >= U as i64;
            if unbond_matured {
                let unbond = self
                    .unbonding
                    .pop_front()?
                    .ok_or_else(|| Error::Coins("Failed to pop unbond".into()))?;
                self.liquid.add(unbond.coins.amount.amount())?;
            } else {
                break;
            }
        }

        Ok(())
    }
}

/// If a delegator is adjusted downward (ie. multiplier less than one), the
/// validator has been slashed and the delegator loses some staked and unbonding
/// shares.
///
/// If a delegator is adjusted upward (ie. multiplier greater than one), the
/// validator has earned some reward. Only the staked coins are adjusted.
impl<S: Symbol, const U: u64> Adjust for Delegator<S, U> {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        use std::cmp::Ordering::*;
        if self.multiplier == 0 {
            self.multiplier = multiplier;
        } else {
            self.multiplier = (self.multiplier * multiplier)?;
        }
        if self.jailed {
            return Ok(());
        }

        let one = Ratio::new(1, 1)?;

        match multiplier.cmp(&one) {
            Greater => {
                let reward = (self.staked.amount * self.multiplier)
                    - (self.staked.amount * (self.multiplier / multiplier))?;
                self.liquid.amount = (self.liquid.amount + reward)?;
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
impl<S: Symbol, const U: u64> Balance<S, Ratio> for Delegator<S, U> {
    fn balance(&self) -> Ratio {
        if self.jailed {
            0.into()
        } else {
            self.staked.amount
        }
    }
}

impl<S: Symbol, const U: u64> Balance<S, Amount> for Delegator<S, U> {
    fn balance(&self) -> Amount {
        self.staked.amount.amount()
    }
}

/// Giving coins to a delegator is used internally in delegation.
impl<S: Symbol, const U: u64> Give<S> for Delegator<S, U> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        // TODO: Make sure it's impossible to delegate to a jailed validator
        self.staked.give(coins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::Context,
        store::{MapStore, Shared, Store},
    };

    #[derive(State, Debug)]
    struct Simp(());
    impl Symbol for Simp {}

    #[test]
    fn staking() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let mut staking: Staking<Simp, 10> = Staking::create(store, Default::default())?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();
        let carol = [2; 32].into();
        let dave = [3; 32].into();

        Context::add(Validators::default());
        Context::add(Time::from_seconds(0));

        staking.delegate(alice, alice, Coin::mint(100))?;
        staking.delegate(bob, bob, Coin::mint(300))?;
        staking.delegate(bob, carol, Coin::mint(100))?;
        staking.delegate(bob, carol, Coin::mint(200))?;
        staking.delegate(bob, dave, Coin::mint(400))?;

        let ctx = Context::resolve::<Validators>().unwrap();

        let total_staked = staking.staked();
        assert_eq!(total_staked, 1100);

        let alice_vp = ctx.updates.get(&alice.bytes).unwrap().power;
        assert_eq!(alice_vp, 100);

        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 1000);

        let alice_self_delegation = staking.get(alice)?.get(alice)?.staked.amount;
        assert_eq!(alice_self_delegation, 100);

        let bob_self_delegation = staking.get(bob)?.get(bob)?.staked.amount;
        assert_eq!(bob_self_delegation, 300);

        let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount;
        assert_eq!(carol_to_bob_delegation, 300);

        let alice_val_balance = staking.get(alice)?.staked();
        assert_eq!(alice_val_balance, 100);

        let bob_val_balance = staking.get(bob)?.staked();
        assert_eq!(bob_val_balance, 1000);

        // Big block rewards, doubling all balances
        staking.give(Coin::mint(600))?;
        staking.give(Coin::mint(500))?;

        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount;
        assert_eq!(alice_liquid, 100);

        let total_staked = staking.staked();
        assert_eq!(total_staked, 1100);

        let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount;
        assert_eq!(carol_to_bob_delegation, 300);
        let carol_to_bob_liquid = staking.get(bob)?.get(carol)?.liquid.amount;
        assert_eq!(carol_to_bob_liquid, 300);

        let bob_val_balance = staking.get(bob)?.staked();
        assert_eq!(bob_val_balance, 1000);

        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 1000);

        // Bob gets slashed 50%
        let slashed_coins = staking.slash(bob, 500)?;
        assert_eq!(slashed_coins.amount, 500);
        slashed_coins.burn();

        // Bob has been jailed and should no longer have any voting power
        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
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
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount;
        assert_eq!(alice_liquid, 100);
        let alice_staked = staking.get(alice)?.get(alice)?.staked.amount;
        assert_eq!(alice_staked, 100);

        // More block reward, but bob's delegators are jailed and should not
        // earn from it
        staking.give(Coin::mint(200))?;
        let alice_val_balance = staking.get(alice)?.staked();
        assert_eq!(alice_val_balance, 100);
        let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount;
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

        Ok(())
    }
}
