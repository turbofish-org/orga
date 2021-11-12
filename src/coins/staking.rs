use super::{Address, Adjust, Amount, Balance, Coin, Give, Pool, Ratio, Share, Symbol, Take};
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::Validators;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
// use tendermint_proto::google::protobuf::Timestamp;

type Delegators<S> = Pool<Address, Delegator<S>, S>;

pub struct Staking<S: Symbol> {
    vp_per_coin: Ratio,
    validators: Pool<Address, Delegators<S>, S>,
}

impl<S: Symbol> State for Staking<S> {
    type Encoding = StakingEncoding<S>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            vp_per_coin: <Ratio as State>::create(store.sub(&[0]), data.vp_per_coin)?,
            validators: <Pool<Address, Delegators<S>, S> as State>::create(
                store.sub(&[1]),
                data.validators,
            )?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(Self::Encoding {
            vp_per_coin: <Ratio as State>::flush(self.vp_per_coin)?,
            validators: <Pool<Address, Delegators<S>, S> as State>::flush(self.validators)?,
        })
    }
}

impl<S: Symbol> From<Staking<S>> for StakingEncoding<S> {
    fn from(staking: Staking<S>) -> Self {
        Self {
            vp_per_coin: staking.vp_per_coin.into(),
            validators: staking.validators.into(),
        }
    }
}

#[derive(Encode, Decode)]
pub struct StakingEncoding<S: Symbol> {
    vp_per_coin: <Ratio as State>::Encoding,
    validators: <Pool<Address, Delegator<S>, S> as State>::Encoding,
}

impl<S: Symbol> Default for StakingEncoding<S> {
    fn default() -> Self {
        Self {
            vp_per_coin: Ratio::new(1, 1).unwrap().into(),
            validators: Default::default(),
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

    pub fn slash(&mut self, val_address: Address, amount: Amount) -> Result<()> {
        let mut validator = self.validators.get_mut(val_address)?;
        validator.take(amount)?.burn();

        Ok(())
    }
}

impl<S: Symbol> Balance<S, Amount> for Staking<S> {
    fn balance(&self) -> Amount {
        self.validators.balance().amount()
    }
}

impl<S: Symbol> Take<S> for Staking<S> {
    type Value = Coin<S>;

    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount = amount.into();
        let one: Amount = 1.into();
        // TODO: Handle withdrawing all coins
        self.vp_per_coin = (self.vp_per_coin / (one - (amount / self.validators.balance())))?;
        let coins = self.validators.take(amount)?;

        Ok(coins)
    }
}

impl<S: Symbol> Give<S> for Staking<S> {
    fn give(&mut self, coins: Coin<S>) -> Result<()> {
        // TODO: Handle giving to empty pool
        let amount = coins.amount;
        let one: Amount = 1.into();
        self.vp_per_coin = (self.vp_per_coin * (one + (amount / self.validators.balance())))?;

        self.validators.give(coins)
    }
}

pub struct Validator<S: Symbol> {
    delegators: Delegators<S>,
}

#[derive(State, Debug)]
pub struct Delegator<S: Symbol> {
    liquid: Coin<S>,
    staked: Share<S>,
    unbonding: Share<S>,
}

/// Taking coins from a delegator means withdrawing liquid coins.
impl<S: Symbol> Take<S> for Delegator<S> {
    type Value = Coin<S>;
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        todo!()
    }
}

/// If a delegator is adjusted downward (ie. multiplier less than one), the
/// validator has been slashed and the delegator loses some staked and unbonding
/// shares.
///
/// If a delegator is adjusted upward (ie. multiplier greater than one), the
/// validator has earned some reward. Only the staked coins are adjusted.
impl<S: Symbol> Adjust for Delegator<S> {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        use std::cmp::Ordering::*;
        let one = Ratio::new(1, 1)?;
        match multiplier.cmp(&one) {
            Greater => {
                self.staked.amount = (self.staked.amount * multiplier)?;
            }
            Less => {
                self.staked.amount = (self.staked.amount * multiplier)?;
                self.unbonding.amount = (self.unbonding.amount * multiplier)?;
            }
            Equal => (),
        }

        Ok(())
    }
}

/// A delegator's balance is its staked coins, since Balance here is used in the
/// parent collection to calculate the validator's voting power.
impl<S: Symbol> Balance<S, Ratio> for Delegator<S> {
    fn balance(&self) -> Ratio {
        self.staked.amount
    }
}

impl<S: Symbol> Balance<S, Amount> for Delegator<S> {
    fn balance(&self) -> Amount {
        self.staked.amount.amount()
    }
}

/// Giving coins to a delegator is used internally in delegation.
impl<S: Symbol> Give<S> for Delegator<S> {
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
        let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();
        let carol = [2; 32].into();
        let dave = [3; 32].into();

        Context::add(Validators::default());

        staking.delegate(alice, alice, Coin::mint(100))?;
        staking.delegate(bob, bob, Coin::mint(300))?;
        staking.delegate(bob, carol, Coin::mint(100))?;
        staking.take(250)?.burn();
        staking.delegate(bob, carol, Coin::mint(200))?;
        staking.delegate(bob, dave, Coin::mint(400))?;

        let ctx = Context::resolve::<Validators>().unwrap();

        let total_staked = staking.balance();
        assert_eq!(total_staked, 850);

        let alice_vp = ctx.updates.get(&alice.bytes).unwrap().power;
        assert_eq!(alice_vp, 100);

        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 1600);

        let alice_self_delegation: Amount = (*staking.validators.get(alice)?.get(alice)?).balance();
        assert_eq!(alice_self_delegation, 50);

        let bob_self_delegation: Amount = (*staking.validators.get(bob)?.get(bob)?).balance();
        assert_eq!(bob_self_delegation, 150);

        let carol_to_bob_delegation: Amount = (*staking.validators.get(bob)?.get(carol)?).balance();
        assert_eq!(carol_to_bob_delegation, 250);

        let alice_val_balance = (*staking.validators.get(alice)?).balance();
        assert_eq!(alice_val_balance, 50);

        let bob_val_balance = (*staking.validators.get(bob)?).balance();
        assert_eq!(bob_val_balance, 800);

        // Bob gets slashed 50%
        {
            let mut bob_val = staking.validators.get_mut(bob)?;
            bob_val.take(400)?.burn();
        }

        let bob_val_balance = (*staking.validators.get(bob)?).balance();
        assert_eq!(bob_val_balance, 400);

        let carol_to_bob_delegation: Amount = (*staking.validators.get(bob)?.get(carol)?).balance();
        assert_eq!(carol_to_bob_delegation, 125);

        let total_staked = staking.balance();
        assert_eq!(total_staked, 450);

        // Big block reward
        staking.give(Coin::mint(450 * 3))?;

        let total_staked = staking.balance();
        assert_eq!(total_staked, 1800);

        let alice_self_delegation: Amount = (*staking.validators.get(alice)?.get(alice)?).balance();
        assert_eq!(alice_self_delegation, 200);

        let carol_to_bob_delegation: Amount = (*staking.validators.get(bob)?.get(carol)?).balance();
        assert_eq!(carol_to_bob_delegation, 500);

        let alice_vp = ctx.updates.get(&alice.bytes).unwrap().power;
        assert_eq!(alice_vp, 100);

        let bob_vp = ctx.updates.get(&bob.bytes).unwrap().power;
        assert_eq!(bob_vp, 1600);

        Ok(())
    }
}
