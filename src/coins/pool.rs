#[cfg(test)]
use mutagen::mutate;

use super::{Adjust, Amount, Balance, Give, RatioEncoding, Symbol, Take};
use super::{Coin, Ratio};
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
    multiplier: Ratio,
    pub(crate) total: Ratio,
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
            multiplier: Ratio::create(store.sub(&[0]), data.multiplier)?,
            total: Ratio::create(store.sub(&[1]), data.total)?,
            symbol: PhantomData,
            map: Map::<_, _, _>::create(store.sub(&[2]), ())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        self.map.flush()?;
        Ok(Self::Encoding {
            multiplier: self.multiplier.flush()?,
            total: self.total.flush()?,
        })
    }
}

#[derive(Encode, Decode)]
pub struct PoolEncoding {
    multiplier: RatioEncoding,
    total: RatioEncoding,
}

impl Default for PoolEncoding {
    fn default() -> Self {
        PoolEncoding {
            multiplier: Ratio::new(1, 1).unwrap().into(),
            total: Ratio::new(0, 1).unwrap().into(),
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
            multiplier: pool.multiplier.into(),
            total: pool.total.into(),
        }
    }
}

impl<K, V, S> Balance<S, Ratio> for Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    fn balance(&self) -> Ratio {
        self.total
    }
}

impl<K, V, S> Adjust for Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        self.multiplier = (self.multiplier * multiplier)?;
        self.total = (self.total * multiplier)?;

        Ok(())
    }
}

#[derive(State)]
pub struct Entry<T, S>
where
    T: State,
    S: Symbol,
{
    last_multiplier: Ratio,
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
    V: State + Adjust + Balance<S, Ratio>,
    V::Encoding: Default,
{
    #[cfg_attr(test, mutate)]
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        let mut child = self.map.entry(key)?.or_default()?;
        let mut entry = child.get_mut();
        if *entry.last_multiplier.0.numer() == 0 {
            entry.last_multiplier = self.multiplier;
        }

        if entry.last_multiplier.0 != self.multiplier.0 {
            let adjustment = (self.multiplier / entry.last_multiplier)?;
            entry.inner.adjust(adjustment)?;
            entry.last_multiplier = self.multiplier;
        }
        let initial_balance = entry.inner.balance();

        Ok(ChildMut {
            parent_total: &mut self.total,
            entry: child,
            initial_balance,
            _symbol: PhantomData,
        })
    }

    #[cfg_attr(test, mutate)]
    pub fn get(&self, key: K) -> Result<Child<V, S>> {
        let entry = self.map.get_or_default(key)?;
        Child::new(entry, self.multiplier)
    }
}

pub type IterEntry<'a, K, V, S> = Result<(MapRef<'a, K>, Child<'a, V, S>)>;

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Decode + Terminated + Clone + Next,
    V: State + Adjust + Balance<S, Ratio>,
    V::Encoding: Default,
{
    #[cfg_attr(test, mutate)]
    pub fn range<B>(&self, bounds: B) -> Result<impl Iterator<Item = IterEntry<K, V, S>>>
    where
        B: RangeBounds<K>,
    {
        Ok(self.map.range(bounds)?.map(move |entry| {
            let entry = entry?;
            let child = Child::new(entry.1, self.multiplier)?;
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
        if self.total == Ratio::new(0, 1)? {
            return Err(Error::Coins("Cannot add directly to an empty pool".into()));
        }

        let amount_ratio: Ratio = coin.amount.into();

        if self.total.0 > 0.into() {
            let base: Amount = 1.into();
            let increase = base + (amount_ratio / self.total)?;
            self.multiplier = (self.multiplier * increase)?;
        } else {
            self.multiplier = Ratio::new(1, 1)?;
        }

        self.total = (self.total + amount_ratio)?;

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
        let amount_ratio: Ratio = amount.into();
        let base: Ratio = 1.into();
        let decrease = base - (amount_ratio / self.total);
        self.total = (self.total - amount_ratio)?;
        self.multiplier = (self.multiplier * decrease)?;

        Ok(Coin::mint(amount))
    }
}

pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Ratio> + Adjust,
    V::Encoding: Default,
{
    parent_total: &'a mut Ratio,
    entry: MapChildMut<'a, K, UnsafeCell<Entry<V, S>>>,
    initial_balance: Ratio,
    _symbol: PhantomData<S>,
}

impl<'a, K, V, S> Drop for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Ratio> + Adjust,
    V::Encoding: Default,
{
    fn drop(&mut self) {
        use std::cmp::Ordering::*;
        let start_balance = self.initial_balance;
        let end_balance = self.entry.get_mut().balance();
        match end_balance.cmp(&start_balance) {
            Greater => {
                *self.parent_total = (*self.parent_total + (end_balance - start_balance))
                    .result()
                    .expect("Overflow");
            }
            Less => {
                let prev_total = *self.parent_total;
                *self.parent_total = (prev_total - (start_balance - end_balance))
                    .result()
                    .expect("Overflow");
            }
            Equal => {}
        };
    }
}

impl<'a, K, V, S> Deref for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance<S, Ratio> + Adjust,
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
    V: State + Balance<S, Ratio> + Adjust,
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
        V: State + Balance<S, Ratio> + Adjust,
        V::Encoding: Default,
    {
        entry: MapRef<'a, UnsafeCell<Entry<V, S>>>,
        _symbol: PhantomData<S>,
    }

    impl<'a, V, S> Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Ratio> + Adjust,
        V::Encoding: Default,
    {
        #[cfg_attr(test, mutate)]
        pub fn new(
            entry_ref: MapRef<'a, UnsafeCell<Entry<V, S>>>,
            current_multiplier: Ratio,
        ) -> Result<Self> {
            let mut entry = unsafe { &mut *entry_ref.get() };
            let zero: Ratio = 0.into();
            if entry.last_multiplier == zero {
                entry.last_multiplier = current_multiplier;
            }

            if entry.last_multiplier != current_multiplier {
                let adjustment = (current_multiplier / entry.last_multiplier)?;
                entry.inner.adjust(adjustment)?;
                entry.last_multiplier = current_multiplier;
            }

            Ok(Child {
                entry: entry_ref,
                _symbol: PhantomData,
            })
        }
    }

    impl<'a, V, S> Deref for Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance<S, Ratio> + Adjust,
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

    #[derive(Encode, Decode, Debug)]
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
            let alice_balance: Ratio = alice_child.balance();
            let target: Ratio = 0.into();
            assert_eq!(alice_balance, target);
        }

        let pool_balance = pool.balance();
        let target: Ratio = 0.into();
        assert_eq!(pool_balance, target);

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.add(10)?;
        }
        let target: Ratio = 10.into();
        assert_eq!(pool.balance(), target);

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.add(2)?;
        }

        let target: Ratio = 12.into();
        assert_eq!(pool.balance(), target);
        {
            let alice_child = pool.get_mut(alice)?;
            let alice_balance: Ratio = alice_child.balance();
            let target: Ratio = 10.into();
            assert_eq!(alice_balance, target);
        }

        pool.add(12)?;

        {
            let alice_child = pool.get(alice)?;
            let target: Ratio = 20.into();
            let alice_balance: Ratio = alice_child.balance();
            assert_eq!(alice_balance, target);
        }

        let target: Ratio = 24.into();
        assert_eq!(pool.balance(), target);

        let taken_coins = pool.take(6)?;
        taken_coins.burn();

        let target: Ratio = 18.into();
        assert_eq!(pool.balance(), target);
        {
            let alice_child = pool.get_mut(alice)?;
            let target: Ratio = 15.into();
            let alice_balance: Ratio = alice_child.balance();
            assert_eq!(alice_balance, target);
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            let taken_coins = alice_child.take(4)?;
            taken_coins.burn();
        }

        let target: Ratio = 14.into();
        assert_eq!(pool.balance(), target);

        {
            let bob_child = pool.get_mut(bob)?;
            let target: Ratio = 3.into();
            let bob_balance: Ratio = bob_child.balance();
            assert_eq!(bob_balance, target);
        }

        pool.adjust(2.into())?;
        let target: Ratio = 28.into();
        assert_eq!(pool.balance(), target);

        {
            let bob_child = pool.get(bob)?;
            let bob_balance: Ratio = bob_child.balance();
            let target: Ratio = 6.into();
            assert_eq!(bob_balance, target);
        }

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.take(6)?.burn();
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.take(22)?.burn();
        }

        let target: Ratio = 0.into();
        assert_eq!(pool.balance(), target);

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.add(6)?;
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.add(22)?;
        }

        pool.add(1)?;

        {
            let mut bob_child = pool.get_mut(bob)?;
            let taken_coins = bob_child.take(6)?;
            taken_coins.burn();
            let bob_balance: Ratio = bob_child.balance();
            let expected_share_fraction = Ratio::new(3, 14)?;
            assert_eq!(bob_balance, expected_share_fraction);
        }

        Ok(())
    }
}
