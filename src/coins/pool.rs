//! Asset distribution with minimal iteration.
use super::{Amount, Balance, Coin, Decimal, Give, Symbol};
use crate::collections::map::{ChildMut as MapChildMut, Ref as MapRef};
use crate::collections::{Map, Next};
use crate::encoding::{Decode, Encode, Terminated};
use crate::orga;
use crate::state::State;
use crate::{Error, Result};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Drop, RangeBounds};

/// Implementation of [F1 Pool] for efficient coin distribution.
///
/// Values are held in a [Map] internally, but accessed via [Pool::get] or
/// [Pool::get_mut], which return [Child] and [ChildMut] respectively.
///
/// These wrappers ensure up-to-date balances for their inner values to their
/// consumer.
///
/// When a [ChildMut] drops, it will update its parent pool based on any
/// changes.
///
/// [F1 Pool]: https://github.com/cosmos/cosmos-sdk/blob/main/docs/spec/fee_distribution/f1_fee_distr.pdf
#[orga]
pub struct Pool<K, V, S>
where
    K: Terminated + Encode + Decode + Clone + Send + Sync + 'static,
    V: State,
    S: Symbol,
{
    contributions: Decimal,
    rewards: Map<u8, Decimal>,
    shares_issued: Decimal,
    /// Backing map for values in the pool.
    pub map: Map<K, RefCell<Entry<V>>>,
    rewards_this_period: Map<u8, Decimal>,
    last_period_entry: Map<u8, Decimal>,
    #[state(skip)]
    drop_errored: bool,
    symbol: PhantomData<S>,
}

impl<K, V, S> Balance<S, Decimal> for Pool<K, V, S>
where
    K: Terminated + Encode + Decode + Clone + Send + Sync + 'static,
    V: State,
    S: Symbol,
{
    fn balance(&self) -> Result<Decimal> {
        Ok(self.contributions)
    }
}

/// A pool entry which tracks the shares and last update period entry for each
/// rewarded denom.
#[orga]
pub struct Entry<T>
where
    T: State,
{
    shares: Decimal,
    last_update_period_entry: Map<u8, Decimal>,
    inner: T,
}

impl<T: State> Deref for Entry<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: State> DerefMut for Entry<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<K, V, S> Pool<K, V, S>
where
    K: Terminated + Encode + Decode + Clone + Send + Sync + 'static,
    V: State,
    S: Symbol,
{
    fn assert_no_unhandled_drop_err(&self) -> Result<()> {
        if self.drop_errored {
            return Err(Error::Coins("Unhandled pool child drop error".into()));
        }
        Ok(())
    }
}

impl<K, V, S> Pool<K, V, S>
where
    K: Encode + Decode + Terminated + Clone + Send + Sync + 'static,
    V: State + Balance<S, Decimal> + Give<(u8, Amount)> + Default,
    S: Symbol,
{
    /// Mutably access an adjusted value in the pool. Changes will be propagated
    /// to the pool when the returned [ChildMut] drops.
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        self.assert_no_unhandled_drop_err()?;
        let mut child = self.map.entry(key)?.or_default()?;
        let entry = child.get_mut();

        let denoms: Vec<u8> = self
            .rewards_this_period
            .iter()?
            .map(|item| Ok(*item?.0))
            .collect::<Result<Vec<u8>>>()?;

        for reward_entry in self.rewards_this_period.iter()? {
            let (denom_index, reward_this_period) = reward_entry?;
            let mut reward = self.rewards.entry(*denom_index)?.or_default()?;
            *reward = (*reward + *reward_this_period)?;
        }

        let initial_balance = entry.balance()?;

        let mut period_entry_hashmap = BTreeMap::new();
        if self.shares_issued > 0 {
            for denom_index in denoms.iter() {
                let mut last_entry = self.last_period_entry.entry(*denom_index)?.or_default()?;
                let reward_this_period =
                    self.rewards_this_period.entry(*denom_index)?.or_default()?;
                *last_entry = ((*reward_this_period / self.shares_issued) + *last_entry)?;
                period_entry_hashmap.insert(*denom_index, *last_entry);
            }
        }

        for denom_index in denoms.iter() {
            let mut reward_this_period =
                self.rewards_this_period.entry(*denom_index)?.or_default()?;
            *reward_this_period = 0.into();
        }

        Self::adjust_entry(
            self.contributions,
            self.shares_issued,
            period_entry_hashmap,
            entry,
        )?;

        Ok(ChildMut {
            parent_num_tokens: &mut self.contributions,
            parent_shares_issued: &mut self.shares_issued,
            drop_errored: &mut self.drop_errored,
            entry: child,
            initial_balance,
            _symbol: PhantomData,
        })
    }

    fn adjust_entry(
        contributions: Decimal,
        shares_issued: Decimal,
        new_period_entry: BTreeMap<u8, Decimal>,
        entry: &mut Entry<V>,
    ) -> Result<()> {
        if shares_issued > 0 {
            for (denom_index, new_denom_period_entry) in new_period_entry.iter() {
                let mut last_entry = entry
                    .last_update_period_entry
                    .entry(*denom_index)?
                    .or_default()?;
                let delta =
                    ((*new_denom_period_entry - *last_entry) * contributions * entry.shares
                        / shares_issued)?;

                *last_entry = *new_denom_period_entry;
                use std::cmp::Ordering::*;
                match delta.cmp(&0.into()) {
                    Less | Equal => {}
                    Greater => {
                        let amount_to_give = delta.amount()?;
                        if amount_to_give >= 1 {
                            entry.give((*denom_index, amount_to_give))?;
                        }
                    }
                };
            }
        }

        Ok(())
    }

    /// Returns an adjusted view of the entry, but does not modify the
    /// underlying map.
    pub fn get(&self, key: K) -> Result<Child<V, S>> {
        self.assert_no_unhandled_drop_err()?;
        let denoms: Vec<u8> = self
            .rewards_this_period
            .iter()?
            .map(|item| Ok(*item?.0))
            .collect::<Result<Vec<u8>>>()?;

        let mut period_entry_hashmap = BTreeMap::new();
        if self.shares_issued > 0 {
            for denom_index in denoms.iter() {
                let last_entry = self.last_period_entry.get_or_default(*denom_index)?;
                let reward_this_period = self.rewards_this_period.get_or_default(*denom_index)?;
                let updated_last_entry =
                    ((*reward_this_period / self.shares_issued) + *last_entry)?;
                period_entry_hashmap.insert(*denom_index, updated_last_entry);
            }
        }
        let entry = self.map.get_or_default(key)?;
        {
            let mut entry_mut = entry.borrow_mut();
            Self::adjust_entry(
                self.contributions,
                self.shares_issued,
                period_entry_hashmap,
                &mut *entry_mut,
            )?;
        }

        Child::new(entry)
    }
}

/// Represents an entry in the pool iterator, containing a key and a child
/// value.
pub type IterEntry<'a, K, V, S> = Result<(K, Child<'a, V, S>)>;

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Decode + Terminated + Clone + Next + Send + Sync + 'static,
    V: State + Balance<S, Decimal> + Give<(u8, Amount)> + Default,
{
    /// Iterate over a range of entries in the pool.
    pub fn range<B>(&self, bounds: B) -> Result<impl Iterator<Item = IterEntry<K, V, S>>>
    where
        B: RangeBounds<K>,
    {
        Ok(self.map.range(bounds)?.map(move |entry| {
            let (key, _child) = entry?;

            let child = self.get(key.clone())?;
            Ok((key.clone(), child))
        }))
    }

    /// Iterate over all entries in the pool.
    pub fn iter(&self) -> Result<impl Iterator<Item = IterEntry<K, V, S>>> {
        self.range(..)
    }
}

impl<K, V, S, T> Give<Coin<T>> for Pool<K, V, S>
where
    S: Symbol,
    T: Symbol,
    K: Encode + Terminated + Decode + Clone + Send + Sync + 'static,
    V: State,
{
    fn give(&mut self, coin: Coin<T>) -> Result<()> {
        if self.contributions == 0 {
            return Err(Error::Coins("Cannot add directly to an empty pool".into()));
        }

        let mut reward_this_period = self.rewards_this_period.entry(T::INDEX)?.or_default()?;
        *reward_this_period = (*reward_this_period + coin.amount)?;

        Ok(())
    }
}

impl<K, V, S> Give<(u8, Amount)> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Decode + Clone + Send + Sync + 'static,
    V: State,
{
    fn give(&mut self, coin: (u8, Amount)) -> Result<()> {
        if self.contributions == 0 {
            return Err(Error::Coins("Cannot add directly to an empty pool".into()));
        }

        let mut reward_this_period = self.rewards_this_period.entry(coin.0)?.or_default()?;
        *reward_this_period = (*reward_this_period + coin.1)?;

        Ok(())
    }
}

/// A child value in the pool, with a mutable reference to the underlying value.
/// The parent pool will be updated when the [ChildMut] is dropped.
pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone + 'static,
    V: State + Balance<S, Decimal>,
{
    parent_num_tokens: &'a mut Decimal,
    parent_shares_issued: &'a mut Decimal,
    drop_errored: &'a mut bool,
    entry: MapChildMut<'a, K, RefCell<Entry<V>>>,
    initial_balance: Decimal,
    _symbol: PhantomData<S>,
}

impl<'a, K, V, S> Drop for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone + 'static,
    V: State + Balance<S, Decimal>,
{
    fn drop(&mut self) {
        let start_balance = self.initial_balance;
        let end_balance = match self.entry.get_mut().balance() {
            Ok(bal) => bal,
            Err(_err) => {
                *self.drop_errored = true;
                return;
            }
        };
        let balance_change: Decimal = match (end_balance - start_balance).result() {
            Ok(bal) => bal,
            Err(_err) => {
                *self.drop_errored = true;
                return;
            }
        };

        if !balance_change.value.is_zero() {
            let new_shares = if self.parent_num_tokens.value.is_zero() {
                balance_change
            } else {
                match (*self.parent_shares_issued * balance_change / *self.parent_num_tokens)
                    .result()
                {
                    Ok(value) => value,
                    Err(_err) => {
                        *self.drop_errored = true;
                        return;
                    }
                }
            };

            let entry = self.entry.get_mut();

            entry.shares = match (entry.shares + new_shares).result() {
                Ok(value) => value,
                Err(_err) => {
                    *self.drop_errored = true;
                    return;
                }
            };

            *self.parent_num_tokens = match (*self.parent_num_tokens + balance_change).result() {
                Ok(value) => value,
                Err(_err) => {
                    *self.drop_errored = true;
                    return;
                }
            };

            *self.parent_shares_issued = match (*self.parent_shares_issued + new_shares).result() {
                Ok(value) => value,
                Err(_err) => {
                    *self.drop_errored = true;
                    return;
                }
            };
        };
    }
}

impl<'a, K, V, S> Deref for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone + 'static,
    V: State + Balance<S, Decimal>,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        let v = self.entry.as_ptr();
        &unsafe { &*v }.inner
    }
}

impl<'a, K, V, S> DerefMut for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone + 'static,
    V: State + Balance<S, Decimal>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.entry.get_mut()
    }
}

// placed in a separate module to ensure instances only get created via
// `Child::new`
mod child {
    use super::*;

    /// A value in the pool, with a reference to the underlying value.
    pub struct Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Decimal>,
    {
        entry: MapRef<'a, RefCell<Entry<V>>>,
        _symbol: PhantomData<S>,
    }

    impl<'a, V, S> Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Decimal>,
    {
        /// Create a new child value.
        pub fn new(entry_ref: MapRef<'a, RefCell<Entry<V>>>) -> Result<Self> {
            Ok(Child {
                entry: entry_ref,
                _symbol: PhantomData,
            })
        }
    }

    impl<'a, V, S> Deref for Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Decimal>,
    {
        type Target = V;

        fn deref(&self) -> &Self::Target {
            let v = self.entry.as_ptr();
            &unsafe { &*v }.inner
        }
    }
}
pub use child::Child;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coins::{Address, Amount, Share};
    use crate::encoding::{Decode, Encode};
    use crate::orga;

    #[orga]
    #[derive(Clone, Debug)]
    struct Simp;
    impl Symbol for Simp {
        const INDEX: u8 = 0;
        const NAME: &'static str = "SIMP";
    }

    #[test]
    fn order_a() -> Result<()> {
        let mut pool: Pool<Address, Share<Simp>, Simp> = Default::default();

        let alice = Address::from_pubkey([0; 33]);
        let bob = Address::from_pubkey([1; 33]);

        pool.get_mut(alice)?.give(Simp::mint(50))?;
        pool.give(Simp::mint(100))?;
        pool.get_mut(bob)?.give(Simp::mint(50))?;

        assert_eq!(pool.balance()?, 100);
        pool.get_mut(alice)?;
        assert_eq!(pool.balance()?, 200);
        assert_eq!(pool.get(alice)?.amount()?, 150);
        assert_eq!(pool.get(bob)?.amount()?, 50);

        Ok(())
    }

    #[test]
    fn order_b() -> Result<()> {
        let mut pool: Pool<Address, Share<Simp>, Simp> = Default::default();

        let alice = Address::from_pubkey([0; 33]);
        let bob = Address::from_pubkey([1; 33]);

        pool.get_mut(alice)?.give(Simp::mint(50))?;
        pool.get_mut(bob)?.give(Simp::mint(50))?;
        pool.give(Simp::mint(100))?;

        assert_eq!(pool.balance()?, 100);
        assert_eq!(pool.get(alice)?.amount()?, 100);
        assert_eq!(pool.get(bob)?.amount()?, 100);
        pool.get_mut(alice)?;
        pool.get_mut(bob)?;

        Ok(())
    }

    #[derive(State, Encode, Decode, Default)]
    struct SimpAccount {
        liquid: Decimal,
        locked: Decimal,
    }
    impl SimpAccount {
        fn deposit_locked<A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
            self.locked = (self.locked + amount.into())?;
            Ok(())
        }
    }
    impl Give<(u8, Amount)> for SimpAccount {
        fn give(&mut self, value: (u8, Amount)) -> Result<()> {
            self.liquid = (self.liquid + value.1)?;

            Ok(())
        }
    }

    impl Balance<Simp, Decimal> for SimpAccount {
        fn balance(&self) -> Result<Decimal> {
            Ok(self.locked)
        }
    }

    #[test]
    fn dividends() -> Result<()> {
        let mut pool: Pool<Address, SimpAccount, Simp> = Default::default();

        let alice = Address::from_pubkey([0; 33]);
        let bob = Address::from_pubkey([1; 33]);

        pool.get_mut(alice)?.deposit_locked(50)?;
        assert_eq!(pool.contributions, 50);
        assert_eq!(pool.get_mut(alice)?.balance()?, 50);
        pool.give(Simp::mint(100))?;
        assert_eq!(pool.contributions, 50);
        assert_eq!(pool.get_mut(alice)?.balance()?, 50);
        assert_eq!(pool.get_mut(alice)?.liquid, 100);
        assert_eq!(pool.contributions, 50);
        pool.get_mut(bob)?.deposit_locked(50)?;
        pool.give(Simp::mint(100))?;
        pool.get_mut(alice)?;

        assert_eq!(pool.get_mut(alice)?.balance()?, 50);
        assert_eq!(pool.get_mut(alice)?.liquid, 150);
        assert_eq!(pool.get_mut(bob)?.balance()?, 50);
        assert_eq!(pool.get_mut(bob)?.liquid, 50);

        Ok(())
    }

    #[test]
    fn emptied_pool() -> Result<()> {
        use crate::coins::Take;
        let mut pool: Pool<Address, Share<Simp>, Simp> = Default::default();

        let alice = Address::from_pubkey([0; 33]);

        pool.get_mut(alice)?.give(Simp::mint(50))?;
        pool.get_mut(alice)?.take(50)?.burn();

        assert_eq!(pool.balance()?, 0);

        pool.get_mut(alice)?.give(Simp::mint(50))?;
        pool.give(Simp::mint(50))?;
        pool.get_mut(alice)?.take(100)?.burn();
        assert_eq!(pool.balance()?, 0);
        pool.give(Simp::mint(50))
            .expect_err("Should not be able to give to emptied pool");

        Ok(())
    }
}
