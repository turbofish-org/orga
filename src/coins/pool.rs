#[cfg(test)]
use mutagen::mutate;

use super::{decimal::DecimalEncoding, Amount, Balance, Coin, Decimal, Give, Symbol, Take};
use crate::collections::map::{ChildMut as MapChildMut, Ref as MapRef};
use crate::collections::{Map, Next};
use crate::encoding::{Decode, Encode, Terminated};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Drop, RangeBounds};

#[derive(Query)]
pub struct Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    contributions: Decimal,
    rewards: Decimal,
    shares_issued: Decimal,
    symbol: PhantomData<S>,
    map: Map<K, UnsafeCell<Entry<V, S>>>,
    rewards_this_period: Decimal,
    last_period_entry: Decimal,
    maybe_drop_err: Option<Error>,
}

#[derive(State)]
struct PeriodEntry {
    shares_issued: Decimal,
}

impl<K, V, S> State for Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    type Encoding = PoolEncoding;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            contributions: Decimal::create(store.sub(&[0]), data.contributions)?,
            rewards: Decimal::create(store.sub(&[1]), data.rewards)?,
            shares_issued: Decimal::create(store.sub(&[2]), data.shares_issued)?,
            symbol: PhantomData,
            map: Map::<_, _, _>::create(store.sub(&[3]), ())?,
            rewards_this_period: Decimal::create(store.sub(&[4]), data.rewards_this_period)?,
            last_period_entry: Decimal::create(store.sub(&[5]), data.last_period_entry)?,
            maybe_drop_err: None,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.assert_no_unhandled_drop_err()?;
        self.map.flush()?;
        Ok(Self::Encoding {
            contributions: self.contributions.flush()?,
            rewards: self.rewards.flush()?,
            shares_issued: self.shares_issued.flush()?,
            rewards_this_period: self.rewards_this_period.flush()?,
            last_period_entry: self.last_period_entry.flush()?,
        })
    }
}

#[derive(Encode, Decode)]
pub struct PoolEncoding {
    contributions: DecimalEncoding,
    rewards: DecimalEncoding,
    shares_issued: DecimalEncoding,
    rewards_this_period: DecimalEncoding,
    last_period_entry: DecimalEncoding,
}

impl Default for PoolEncoding {
    fn default() -> Self {
        let contributions: Decimal = 0.into();
        let shares_issued: Decimal = 0.into();
        let rewards: Decimal = 0.into();
        let last_period_entry: Decimal = 0.into();
        let rewards_this_period: Decimal = 0.into();
        PoolEncoding {
            contributions: contributions.into(),
            rewards: rewards.into(),
            shares_issued: shares_issued.into(),
            rewards_this_period: rewards_this_period.into(),
            last_period_entry: last_period_entry.into(),
        }
    }
}

impl<K, V, S> From<Pool<K, V, S>> for PoolEncoding
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    fn from(pool: Pool<K, V, S>) -> Self {
        PoolEncoding {
            contributions: pool.contributions.into(),
            rewards: pool.rewards.into(),
            shares_issued: pool.shares_issued.into(),
            rewards_this_period: pool.rewards_this_period.into(),
            last_period_entry: pool.last_period_entry.into(),
        }
    }
}

impl<K, V, S> Balance<S, Decimal> for Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    fn balance(&self) -> Result<Decimal> {
        Ok(self.contributions)
    }
}

#[derive(State)]
pub struct Entry<T, S>
where
    T: State,
    S: Symbol,
{
    shares: Decimal,
    last_update_period_entry: Decimal,
    symbol: PhantomData<S>,
    inner: T,
}

impl<T: State, S: Symbol> Deref for Entry<T, S> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: State, S: Symbol> DerefMut for Entry<T, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<K, V, S> Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    fn assert_no_unhandled_drop_err(&self) -> Result<()> {
        if let Some(err) = &self.maybe_drop_err {
            return Err(Error::Coins(
                "Unhandled pool child drop error: ".to_string() + err.to_string().as_str(),
            ));
        }
        Ok(())
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Balance<S, Decimal> + Give<S> + Take<S>,
    V::Encoding: Default,
{
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        self.assert_no_unhandled_drop_err()?;
        let mut child = self.map.entry(key)?.or_default()?;
        let entry = child.get_mut();

        self.rewards = (self.rewards + self.rewards_this_period)?;
        let initial_balance = entry.balance()?;
        let last_period_entry = self.last_period_entry;
        let new_period_entry = if self.shares_issued > 0 {
            ((self.rewards_this_period / self.shares_issued) + last_period_entry)?
        } else {
            0.into()
        };
        self.last_period_entry = new_period_entry;
        self.rewards_this_period = 0.into();

        Self::adjust_entry(
            self.contributions,
            self.shares_issued,
            new_period_entry,
            entry,
        )?;

        Ok(ChildMut {
            parent_num_tokens: &mut self.contributions,
            parent_shares_issued: &mut self.shares_issued,
            maybe_drop_err: &mut self.maybe_drop_err,
            entry: child,
            initial_balance,
            _symbol: PhantomData,
        })
    }

    fn adjust_entry(
        contributions: Decimal,
        shares_issued: Decimal,
        new_period_entry: Decimal,
        entry: &mut Entry<V, S>,
    ) -> Result<()> {
        if shares_issued > 0 {
            let delta = ((new_period_entry - entry.last_update_period_entry)
                * contributions
                * entry.shares
                / shares_issued)?;
            use std::cmp::Ordering::*;
            match delta.cmp(&0.into()) {
                Less => {
                    let zero: Amount = 0.into();
                    let coins_to_take = (zero - delta)?.amount()?;
                    if coins_to_take >= 1 {
                        entry.take(coins_to_take)?;
                    }
                }
                Greater => {
                    let coins_to_give = delta.amount()?;
                    if coins_to_give >= 1 {
                        entry.give(coins_to_give.into())?;
                    }
                }
                Equal => {}
            };
        }

        entry.last_update_period_entry = new_period_entry;

        Ok(())
    }

    pub fn get(&self, key: K) -> Result<Child<V, S>> {
        self.assert_no_unhandled_drop_err()?;
        let last_period_entry = self.last_period_entry;
        let new_period_entry = if self.shares_issued > 0 {
            ((self.rewards_this_period / self.shares_issued) + last_period_entry)?
        } else {
            0.into()
        };
        let entry = self.map.get_or_default(key)?;
        let entry_mut = unsafe { &mut *entry.get() };
        Self::adjust_entry(
            self.contributions,
            self.shares_issued,
            new_period_entry,
            entry_mut,
        )?;

        Child::new(entry)
    }
}

pub type IterEntry<'a, K, V, S> = Result<(MapRef<'a, K>, Child<'a, V, S>)>;

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Decode + Terminated + Clone + Next,
    V: State + Balance<S, Decimal>,
    V::Encoding: Default,
{
    #[cfg_attr(test, mutate)]
    pub fn range<B>(&self, bounds: B) -> Result<impl Iterator<Item = IterEntry<K, V, S>>>
    where
        B: RangeBounds<K>,
    {
        Ok(self.map.range(bounds)?.map(move |entry| {
            let entry = entry?;
            let child = Child::new(entry.1)?;
            Ok((entry.0, child))
        }))
    }

    #[cfg_attr(test, mutate)]
    pub fn iter(&self) -> Result<impl Iterator<Item = IterEntry<K, V, S>>> {
        self.range(..)
    }
}

impl<K, V, S> Give<S> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    fn give(&mut self, coin: Coin<S>) -> Result<()> {
        if self.contributions == 0 {
            return Err(Error::Coins("Cannot add directly to an empty pool".into()));
        }

        self.rewards_this_period = (self.rewards_this_period + coin.amount)?;

        Ok(())
    }
}

impl<K, V, S> Take<S> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    type Value = Coin<S>;

    fn take<A>(&mut self, amount: A) -> Result<Self::Value>
    where
        A: Into<Amount>,
    {
        let amount = amount.into();

        if amount > self.contributions {
            return Err(Error::Coins(
                "Cannot take more than the pool contains".into(),
            ));
        }

        self.contributions = (self.contributions - amount)?;

        Ok(Coin::mint(amount))
    }
}

pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Decimal>,
    V::Encoding: Default,
{
    parent_num_tokens: &'a mut Decimal,
    parent_shares_issued: &'a mut Decimal,
    maybe_drop_err: &'a mut Option<Error>,
    entry: MapChildMut<'a, K, UnsafeCell<Entry<V, S>>>,
    initial_balance: Decimal,
    _symbol: PhantomData<S>,
}

impl<'a, K, V, S> Drop for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Decimal>,
    V::Encoding: Default,
{
    fn drop(&mut self) {
        let start_balance = self.initial_balance;
        let end_balance = match self.entry.get_mut().balance() {
            Ok(bal) => bal,
            Err(err) => {
                self.maybe_drop_err.replace(err);
                return;
            }
        };
        let balance_change: Decimal = match (end_balance - start_balance).result() {
            Ok(bal) => bal,
            Err(err) => {
                self.maybe_drop_err.replace(err);
                return;
            }
        };

        debug_assert_eq!(
            self.parent_num_tokens.0.is_sign_positive(),
            self.parent_num_tokens.0.is_sign_positive()
        );
        if !balance_change.0.is_zero() {
            let new_shares = if self.parent_num_tokens.0.is_zero() {
                balance_change
            } else {
                match (*self.parent_shares_issued * balance_change / *self.parent_num_tokens)
                    .result()
                {
                    Ok(value) => value,
                    Err(err) => {
                        self.maybe_drop_err.replace(err);
                        return;
                    }
                }
            };

            let mut entry = self.entry.get_mut();

            entry.shares = match (entry.shares + new_shares).result() {
                Ok(value) => value,
                Err(err) => {
                    self.maybe_drop_err.replace(err);
                    return;
                }
            };

            *self.parent_num_tokens = match (*self.parent_num_tokens + balance_change).result() {
                Ok(value) => value,
                Err(err) => {
                    self.maybe_drop_err.replace(err);
                    return;
                }
            };

            *self.parent_shares_issued = match (*self.parent_shares_issued + new_shares).result() {
                Ok(value) => value,
                Err(err) => {
                    self.maybe_drop_err.replace(err);
                    return;
                }
            };
        };
    }
}

impl<'a, K, V, S> Deref for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Decimal>,
    V::Encoding: Default,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        let v = self.entry.get();
        &unsafe { &*v }.inner
    }
}

impl<'a, K, V, S> DerefMut for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Decimal>,
    V::Encoding: Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.entry.get_mut()
    }
}

// placed in a separate module to ensure instances only get created via
// `Child::new`
mod child {
    use super::*;

    pub struct Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Decimal>,
        V::Encoding: Default,
    {
        entry: MapRef<'a, UnsafeCell<Entry<V, S>>>,
        _symbol: PhantomData<S>,
    }

    impl<'a, V, S> Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Decimal>,
        V::Encoding: Default,
    {
        #[cfg_attr(test, mutate)]
        pub fn new(entry_ref: MapRef<'a, UnsafeCell<Entry<V, S>>>) -> Result<Self> {
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
        V::Encoding: Default,
    {
        type Target = V;

        fn deref(&self) -> &Self::Target {
            let v = self.entry.get();
            &unsafe { &*v }.inner
        }
    }
}
pub use child::Child;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coins::{Address, Share};
    use crate::encoding::{Decode, Encode};
    use crate::store::{MapStore, Shared, Store};

    #[derive(Encode, Decode, Debug, Clone)]
    struct Simp;
    impl Symbol for Simp {}

    impl State for Simp {
        type Encoding = Self;

        fn create(_: Store, data: Self::Encoding) -> Result<Self> {
            Ok(data)
        }

        fn flush(self) -> Result<Self::Encoding> {
            Ok(self)
        }
    }

    #[test]
    fn order_a() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, Share<Simp>, Simp>::create(store, enc)?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();

        pool.get_mut(alice)?.give(50.into())?;
        pool.give(100.into())?;
        pool.get_mut(bob)?.give(50.into())?;

        assert_eq!(pool.balance()?, 100);
        pool.get_mut(alice)?;
        assert_eq!(pool.balance()?, 200);
        assert_eq!(pool.get(alice)?.amount()?, 150);
        assert_eq!(pool.get(bob)?.amount()?, 50);

        Ok(())
    }

    #[test]
    fn order_b() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, Share<Simp>, Simp>::create(store, enc)?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();

        pool.get_mut(alice)?.give(50.into())?;
        pool.get_mut(bob)?.give(50.into())?;
        pool.give(100.into())?;

        assert_eq!(pool.balance()?, 100);
        assert_eq!(pool.get(alice)?.amount()?, 100);
        assert_eq!(pool.get(bob)?.amount()?, 100);
        pool.get_mut(alice)?;
        pool.get_mut(bob)?;

        Ok(())
    }

    #[derive(State)]
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
    impl Give<Simp> for SimpAccount {
        fn give(&mut self, value: Coin<Simp>) -> Result<()> {
            self.liquid = (self.liquid + value.amount)?;

            Ok(())
        }
    }
    impl Take<Simp> for SimpAccount {
        type Value = Coin<Simp>;
        fn take<A: Into<Amount>>(&mut self, _amount: A) -> Result<Coin<Simp>> {
            unimplemented!()
        }
    }
    impl Balance<Simp, Decimal> for SimpAccount {
        fn balance(&self) -> Result<Decimal> {
            Ok(self.locked)
        }
    }

    #[test]
    fn dividends() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, SimpAccount, Simp>::create(store, enc)?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();

        pool.get_mut(alice)?.deposit_locked(50)?;
        assert_eq!(pool.contributions, 50);
        assert_eq!(pool.get_mut(alice)?.balance()?, 50);
        pool.give(100.into())?;
        assert_eq!(pool.contributions, 50);
        assert_eq!(pool.get_mut(alice)?.balance()?, 50);
        assert_eq!(pool.get_mut(alice)?.liquid, 100);
        assert_eq!(pool.contributions, 50);
        pool.get_mut(bob)?.deposit_locked(50)?;
        pool.give(100.into())?;
        pool.get_mut(alice)?;

        assert_eq!(pool.get_mut(alice)?.balance()?, 50);
        assert_eq!(pool.get_mut(alice)?.liquid, 150);
        assert_eq!(pool.get_mut(bob)?.balance()?, 50);
        assert_eq!(pool.get_mut(bob)?.liquid, 50);

        Ok(())
    }

    #[test]
    fn emptied_pool() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, Share<Simp>, Simp>::create(store, enc)?;

        let alice = [0; 32].into();

        pool.get_mut(alice)?.give(50.into())?;
        pool.get_mut(alice)?.take(50)?.burn();

        assert_eq!(pool.balance()?, 0);

        pool.get_mut(alice)?.give(50.into())?;
        pool.give(50.into())?;
        pool.get_mut(alice)?.take(100)?.burn();
        assert_eq!(pool.balance()?, 0);
        pool.give(50.into())
            .expect_err("Should not be able to give to emptied pool");

        Ok(())
    }
}
