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
}

impl<S: Symbol, const U: u64> State for Staking<S, U> {
    type Encoding = StakingEncoding<S, U>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            vp_per_coin: <Ratio as State>::create(store.sub(&[0]), data.vp_per_coin)?,
            validators: State::create(store.sub(&[1]), data.validators)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(Self::Encoding {
            vp_per_coin: <Ratio as State>::flush(self.vp_per_coin)?,
            validators: self.validators.flush()?,
        })
    }
}

impl<S: Symbol, const U: u64> From<Staking<S, U>> for StakingEncoding<S, U> {
    fn from(staking: Staking<S, U>) -> Self {
        Self {
            vp_per_coin: staking.vp_per_coin.into(),
            validators: staking.validators.into(),
        }
    }
}

#[derive(Encode, Decode)]
pub struct StakingEncoding<S: Symbol, const U: u64> {
    vp_per_coin: <Ratio as State>::Encoding,
    validators: <Pool<Address, Validator<S, U>, S> as State>::Encoding,
}

impl<S: Symbol, const U: u64> Default for StakingEncoding<S, U> {
    fn default() -> Self {
        Self {
            vp_per_coin: Ratio::new(1, 1).unwrap().into(),
            validators: Default::default(),
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
        let mut delegator = validator.get_mut(delegator_address)?;
        delegator.give(coins)?;
        drop(delegator);
        let voting_power = (validator.balance() * self.vp_per_coin)?.to_integer();
        drop(validator);

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(val_address, voting_power);

        Ok(())
    }

    pub fn slash<A: Into<Amount>>(&mut self, val_address: Address, amount: A) -> Result<Coin<S>> {
        let amount = amount.into();
        let balance_before = self.balance();
        let mut validator = self.validators.get_mut(val_address)?;
        let slashed_coins = validator.slash(amount)?;
        drop(validator);
        let balance_after = self.balance();

        self.vp_per_coin = (self.vp_per_coin * (balance_after / balance_before))?;

        self.context::<Validators>()
            .ok_or_else(|| Error::Coins("No Validators context available".into()))?
            .set_voting_power(val_address, 0);

        Ok(slashed_coins)
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

impl<S: Symbol, const U: u64> Balance<S, Amount> for Staking<S, U> {
    fn balance(&self) -> Amount {
        self.validators.balance().amount()
    }
}

impl<S: Symbol, const U: u64> Give<S> for Staking<S, U> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        // TODO: Handle giving to empty pool
        let amount = coins.amount;
        let one: Amount = 1.into();
        self.vp_per_coin = (self.vp_per_coin * (one + (amount / self.validators.balance())))?;

        self.validators.give(coins)
    }
}

#[derive(State)]
pub struct Validator<S: Symbol, const U: u64> {
    jailed: bool,
    delegators: Delegators<S, U>,
    jailed_coins: Amount,
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

    fn slash(&mut self, amount: Amount) -> Result<Coin<S>> {
        let slashed_coins = self.take(amount)?;
        self.jailed = true;
        let delegator_keys: Vec<Address> = self
            .delegators
            .iter()?
            .filter_map(|entry| match entry {
                Err(_e) => None, // TODO: Handle error
                Ok((k, _v)) => Some(*k),
            })
            .collect();

        delegator_keys.iter().try_for_each(|k| -> Result<()> {
            let mut delegator = self.delegators.get_mut(*k)?;
            delegator.jailed = true;
            Ok(())
        })?;

        Ok(slashed_coins)
    }

    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Coin<S>> {
        self.delegators.take(amount)
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
            self.delegators.balance()
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
    coins: Coin<S>,
    start_seconds: i64,
}
#[derive(State)]
pub struct Delegator<S: Symbol, const U: u64> {
    liquid: Coin<S>,
    staked: Share<S>,
    jailed: bool,
    unbonding: Deque<Unbond<S>>,
}

impl<S: Symbol, const U: u64> Delegator<S, U> {
    pub fn unbond<A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        let amount = amount.into();
        let coins = self.staked.take(amount)?;
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
                self.liquid.add(unbond.coins.amount)?;
            } else {
                break;
            }
        }

        Ok(())
    }
}

/// Taking coins from a delegator means withdrawing liquid coins.
impl<S: Symbol, const U: u64> Take<S> for Delegator<S, U> {
    type Value = Coin<S>;
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount = amount.into();
        self.process_unbonds()?;
        self.liquid.take(amount)
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
        if self.jailed {
            return Ok(());
        }
        let one = Ratio::new(1, 1)?;

        match multiplier.cmp(&one) {
            Greater => {
                self.staked.amount = (self.staked.amount * multiplier)?;
            }
            Less => {
                self.staked.amount = (self.staked.amount * multiplier)?;
                // self.unbonding.amount = (self.unbonding.amount * multiplier)?;
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

        let total_staked = staking.balance();
        assert_eq!(total_staked, 1100);

        let alice_vp = ctx.updates.get(&alice.bytes).unwrap().power;
        assert_eq!(alice_vp, 100);

        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 1000);

        let alice_self_delegation: Amount = (*staking.get(alice)?.delegators.get(alice)?).balance();
        assert_eq!(alice_self_delegation, 100);

        let bob_self_delegation: Amount = (*staking.get(bob)?.delegators.get(bob)?).balance();
        assert_eq!(bob_self_delegation, 300);

        let carol_to_bob_delegation: Amount = (*staking.get(bob)?.delegators.get(carol)?).balance();
        assert_eq!(carol_to_bob_delegation, 300);

        let alice_val_balance = (*staking.get(alice)?).balance();
        assert_eq!(alice_val_balance, 100);

        let bob_val_balance = (*staking.get(bob)?).balance();
        assert_eq!(bob_val_balance, 1000);

        // Big block rewards, doubling all balances
        staking.give(Coin::mint(600))?;
        staking.give(Coin::mint(500))?;

        let total_staked = staking.balance();
        assert_eq!(total_staked, 2200);

        let carol_to_bob_delegation: Amount = (*staking.get(bob)?.delegators.get(carol)?).balance();
        assert_eq!(carol_to_bob_delegation, 600);

        let bob_val_balance = (*staking.get(bob)?).balance();
        assert_eq!(bob_val_balance, 2000);

        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 1000);

        // Bob gets slashed 50%
        let slashed_coins = staking.slash(bob, 1000)?;
        assert_eq!(slashed_coins.amount, 1000);
        slashed_coins.burn();

        // Bob has been jailed and should no longer have any voting power
        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 0);

        // Bob's staked coins should no longer be present in the global staking
        // balance
        let total_staked = staking.balance();
        assert_eq!(total_staked, 200);

        // Carol can still withdraw her 300 coins from Bob's jailed validator
        {
            staking.get_mut(bob)?.get_mut(carol)?.unbond(300)?;
            staking
                .get_mut(bob)?
                .get_mut(carol)?
                .take(300)
                .expect_err("Should not be able to take coins before unbonding period has elapsed");
            Context::add(Time::from_seconds(10));
            let alice_recovered_coins = staking.get_mut(bob)?.get_mut(carol)?.take(300)?;

            assert_eq!(alice_recovered_coins.amount, 300);
        }

        {
            // Bob withdraws a third of his self-delegation
            staking.get_mut(bob)?.get_mut(bob)?.unbond(100)?;
            Context::add(Time::from_seconds(20));
            let bob_recovered_coins = staking.get_mut(bob)?.get_mut(bob)?.take(100)?;
            assert_eq!(bob_recovered_coins.amount, 100);
            staking
                .get_mut(bob)?
                .get_mut(bob)?
                .unbond(201)
                .expect_err("Should not be able to unbond more than we have staked");

            staking.get_mut(bob)?.get_mut(bob)?.unbond(50)?;
            Context::add(Time::from_seconds(30));
            staking
                .get_mut(bob)?
                .get_mut(bob)?
                .take(51)
                .expect_err("Should not be able to take more than we have unbonded");
            staking.get_mut(bob)?.get_mut(bob)?.take(50)?.burn();
        }

        let total_staked = staking.balance();
        assert_eq!(total_staked, 200);

        // More block reward, but bob's delegators are jailed and should not
        // earn from it
        staking.give(Coin::mint(200))?;

        let total_staked = staking.balance();
        assert_eq!(total_staked, 400);

        let alice_val_balance = (*staking.get(alice)?).balance();
        assert_eq!(alice_val_balance, 400);

        staking
            .get_mut(bob)?
            .get_mut(dave)?
            .unbond(401)
            .expect_err("Dave should only have 400 unbondable coins");

        staking.get_mut(bob)?.get_mut(dave)?.unbond(400)?;

        Ok(())
    }
}
