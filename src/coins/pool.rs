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
    num_tokens: Decimal,
    shares_issued: Decimal,
    symbol: PhantomData<S>,
    map: Map<K, UnsafeCell<Entry<V, S>>>,
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
            num_tokens: Decimal::create(store.sub(&[0]), data.num_tokens)?,
            shares_issued: Decimal::create(store.sub(&[1]), data.shares_issued)?,
            symbol: PhantomData,
            map: Map::<_, _, _>::create(store.sub(&[2]), ())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.map.flush()?;
        Ok(Self::Encoding {
            num_tokens: self.num_tokens.flush()?,
            shares_issued: self.shares_issued.flush()?,
        })
    }
}

#[derive(Encode, Decode)]
pub struct PoolEncoding {
    num_tokens: DecimalEncoding,
    shares_issued: DecimalEncoding,
}

impl Default for PoolEncoding {
    fn default() -> Self {
        let num_tokens: Decimal = 0.into();
        let shares_issued: Decimal = 0.into();
        PoolEncoding {
            num_tokens: num_tokens.into(),
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
            num_tokens: pool.num_tokens.into(),
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
        Ok(self.num_tokens)
    }
}

#[derive(State)]
pub struct Entry<T, S>
where
    T: State,
    S: Symbol,
{
    shares: Decimal,
    amount_given: Decimal,
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
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Balance<S, Decimal> + Give<S> + Take<S>,
    V::Encoding: Default,
{
    #[cfg_attr(test, mutate)]
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        let num_tokens = self.num_tokens;
        let shares_issued = self.shares_issued;
        let mut child = self.map.entry(key)?.or_default()?;
        let entry = child.get_mut();

        Self::adjust_entry(num_tokens, shares_issued, entry)?;

        let initial_balance = entry.balance()?;

        Ok(ChildMut {
            parent_num_tokens: &mut self.num_tokens,
            parent_shares_issued: &mut self.shares_issued,
            entry: child,
            initial_balance,
            _symbol: PhantomData,
        })
    }

    fn adjust_entry(
        num_tokens: Decimal,
        shares_issued: Decimal,
        entry: &mut Entry<V, S>,
    ) -> Result<()> {
        if shares_issued > 0 {
            let rightful_coins = (num_tokens * entry.shares / shares_issued)?;
            let given = entry.amount_given;

            use std::cmp::Ordering::*;
            match given.cmp(&rightful_coins) {
                Greater => {
                    let coins_to_take = (given - rightful_coins)?.amount()?;
                    entry.take(coins_to_take)?;
                    entry.amount_given = (given - coins_to_take)?;
                }
                Less => {
                    let coins_to_give = (rightful_coins - given)?.amount()?;
                    entry.give(coins_to_give.into())?;
                    entry.amount_given = (given + coins_to_give)?;
                }
                Equal => {}
            };
        }

        Ok(())
    }

    #[cfg_attr(test, mutate)]
    pub fn get(&self, key: K) -> Result<Child<V, S>> {
        let num_tokens = self.num_tokens;
        let shares_issued = self.shares_issued;
        let entry = self.map.get_or_default(key)?;
        let entry_mut = unsafe { &mut *entry.get() };

        Self::adjust_entry(num_tokens, shares_issued, entry_mut)?;

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
        if self.num_tokens == 0 {
            return Err(Error::Coins("Cannot add directly to an empty pool".into()));
        }

        self.num_tokens = (self.num_tokens + coin.amount)?;

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

        if amount > self.num_tokens {
            return Err(Error::Coins(
                "Cannot take more than the pool contains".into(),
            ));
        }

        self.num_tokens = (self.num_tokens - amount)?;

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
        let end_balance = self.entry.get_mut().balance().unwrap();
        let balance_change: Decimal = (end_balance - start_balance)
            .result()
            .expect("Overflow calculating balance change");

        debug_assert_eq!(
            self.parent_num_tokens.0.is_sign_positive(),
            self.parent_num_tokens.0.is_sign_positive()
        );

        if !balance_change.0.is_zero() {
            let new_shares = if self.parent_num_tokens.0.is_zero() {
                balance_change
            } else {
                (*self.parent_shares_issued * balance_change / *self.parent_num_tokens)
                    .result()
                    .expect("Error calculating new pool shares in pool child drop")
            };

            let mut entry = self.entry.get_mut();
            entry.amount_given = (entry.amount_given + balance_change)
                .result()
                .expect("Overflow adding to entry amount_given");

            entry.shares = (entry.shares + new_shares)
                .result()
                .expect("Overflow changing pool child entry shares");

            *self.parent_num_tokens = (*self.parent_num_tokens + balance_change)
                .result()
                .expect("Overflow changing parent pool num_tokens");

            *self.parent_shares_issued = (*self.parent_shares_issued + new_shares)
                .result()
                .expect("Overflow changing parent pool shares_issued");
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
    fn simple_pool() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = Default::default();
        let mut pool = Pool::<Address, Share<Simp>, Simp>::create(store, enc)?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();

        pool.add(1)
            .expect_err("Should not be able to add to empty pool");

        {
            let alice_child = pool.get(alice)?;
            let alice_balance: Decimal = alice_child.balance()?;
            let target: Decimal = 0.into();
            assert_eq!(alice_balance, target);
        }

        let pool_balance = pool.balance()?;
        let target: Decimal = 0.into();
        assert_eq!(pool_balance, target);

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.add(100)?;
        }
        let target: Decimal = 100.into();
        assert_eq!(pool.balance()?, target);

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.add(20)?;
        }

        let target: Decimal = 120.into();
        assert_eq!(pool.balance()?, target);
        {
            let alice_child = pool.get_mut(alice)?;
            let alice_balance: Decimal = alice_child.balance()?;
            let target: Decimal = 100.into();
            assert_eq!(alice_balance, target);
        }

        pool.add(120)?;

        {
            let alice_child = pool.get_mut(alice)?;
            let target: Decimal = 200.into();
            let alice_balance: Decimal = alice_child.balance()?;
            assert_eq!(alice_balance, target);
        }

        let target: Decimal = 240.into();
        assert_eq!(pool.balance()?, target);

        let taken_coins = pool.take(60)?;
        taken_coins.burn();

        let target: Decimal = 180.into();
        assert_eq!(pool.balance()?, target);
        {
            let alice_child = pool.get_mut(alice)?;
            let target: Decimal = 150.into();
            let alice_balance: Decimal = alice_child.balance()?;
            assert_eq!(alice_balance, target);
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            let taken_coins = alice_child.take(40)?;
            taken_coins.burn();
        }

        let target: Decimal = 140.into();
        assert_eq!(pool.balance()?, target);

        {
            let bob_child = pool.get_mut(bob)?;
            let target: Decimal = 30.into();
            let bob_balance: Decimal = bob_child.balance()?;
            assert_eq!(bob_balance, target);
        }

        pool.add(140)?;

        let target: Decimal = 280.into();
        assert_eq!(pool.balance()?, target);

        {
            let bob_child = pool.get(bob)?;
            let bob_balance: Decimal = bob_child.balance()?;
            let target: Decimal = 60.into();
            assert_eq!(bob_balance, target);
        }

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.take(60)?.burn();
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.take(220)?.burn();
        }

        let target: Decimal = 0.into();
        assert_eq!(pool.balance()?, target);

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.add(60)?;
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.add(220)?;
        }

        pool.add(10)?;

        {
            let mut bob_child = pool.get_mut(bob)?;
            let taken_coins = bob_child.take(60)?;
            taken_coins.burn();
            let bob_balance = bob_child.amount()?;
            let target: Amount = 2.into();
            assert_eq!(bob_balance, target);
        }

        Ok(())
    }
}
