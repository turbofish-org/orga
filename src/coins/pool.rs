#[cfg(test)]
use mutagen::mutate;

use super::{decimal::DecimalEncoding, Amount, Balance, Coin, Decimal, Give, Symbol};
use crate::collections::map::{ChildMut as MapChildMut, Ref as MapRef};
use crate::collections::{Map, Next};
use crate::encoding::{Decode, Encode, Terminated};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use serde::Serialize;
use std::cell::RefCell;
use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Drop, RangeBounds};

#[derive(Query, Serialize)]
#[serde(bound(
    serialize = "K: Serialize + Decode + Clone + Next, V: Serialize",
    deserialize = "K: Deserialize<'de> + Decode + Clone, V: Deserialize<'de>",
))]
pub struct Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    contributions: Decimal,
    rewards: Map<u8, Decimal>,
    symbol: PhantomData<S>,
    shares_issued: Decimal,
    map: Map<K, RefCell<Entry<V>>>,
    rewards_this_period: Map<u8, Decimal>,
    last_period_entry: Map<u8, Decimal>,
    #[serde(skip)]
    maybe_drop_err: Option<Error>,
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
            rewards: State::create(store.sub(&[1]), ())?,
            symbol: PhantomData,
            shares_issued: Decimal::create(store.sub(&[2]), data.shares_issued)?,
            map: Map::<_, _, _>::create(store.sub(&[3]), ())?,
            rewards_this_period: State::create(store.sub(&[4]), ())?,
            last_period_entry: State::create(store.sub(&[5]), ())?,
            maybe_drop_err: None,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.assert_no_unhandled_drop_err()?;
        self.map.flush()?;
        self.rewards.flush()?;
        self.rewards_this_period.flush()?;
        self.last_period_entry.flush()?;
        Ok(Self::Encoding {
            contributions: self.contributions.flush()?,
            shares_issued: self.shares_issued.flush()?,
        })
    }
}

#[derive(Encode, Decode)]
pub struct PoolEncoding {
    contributions: DecimalEncoding,
    shares_issued: DecimalEncoding,
}

impl Default for PoolEncoding {
    fn default() -> Self {
        let contributions: Decimal = 0.into();
        let shares_issued: Decimal = 0.into();
        PoolEncoding {
            contributions: contributions.into(),
            shares_issued: shares_issued.into(),
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
            shares_issued: pool.shares_issued.into(),
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

#[derive(State, Serialize)]
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
    K: Encode + Terminated + Clone,
    V: State + Balance<S, Decimal> + Give<(u8, Amount)>,
    V::Encoding: Default,
    S: Symbol,
{
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
            maybe_drop_err: &mut self.maybe_drop_err,
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
        let entry_mut = unsafe { &mut *entry.as_ptr() };
        Self::adjust_entry(
            self.contributions,
            self.shares_issued,
            period_entry_hashmap,
            entry_mut,
        )?;

        Child::new(entry)
    }
}

pub type IterEntry<'a, K, V, S> = Result<(K, Child<'a, V, S>)>;

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Decode + Terminated + Clone + Next,
    V: State + Balance<S, Decimal> + Give<(u8, Amount)>,
    V::Encoding: Default,
{
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

    pub fn iter(&self) -> Result<impl Iterator<Item = IterEntry<K, V, S>>> {
        self.range(..)
    }
}

impl<K, V, S, T> Give<Coin<T>> for Pool<K, V, S>
where
    S: Symbol,
    T: Symbol,
    K: Encode + Terminated,
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
    K: Encode + Terminated,
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
    entry: MapChildMut<'a, K, RefCell<Entry<V>>>,
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
        let v = self.entry.as_ptr();
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
        entry: MapRef<'a, RefCell<Entry<V>>>,
        _symbol: PhantomData<S>,
    }

    impl<'a, V, S> Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Decimal>,
        V::Encoding: Default,
    {
        #[cfg_attr(test, mutate)]
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
        V::Encoding: Default,
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
    use crate::store::{MapStore, Shared, Store};

    #[derive(Encode, Decode, Debug, Clone)]
    struct Simp;
    impl Symbol for Simp {
        const INDEX: u8 = 0;
    }

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
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, Share<Simp>, Simp>::create(store, enc)?;

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
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, SimpAccount, Simp>::create(store, enc)?;

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
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, Share<Simp>, Simp>::create(store, enc)?;

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
