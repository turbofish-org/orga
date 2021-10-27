#[cfg(test)]
use mutagen::mutate;

use super::Ratio;
use super::{Adjust, Amount, Balance, Give, Symbol, Take};
use crate::collections::map::{ChildMut as MapChildMut, Ref as MapRef};
use crate::collections::{Map, Next};
use crate::encoding::{Decode, Encode, Terminated};
use crate::query::Query;
use crate::state::State;
use crate::Result;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Drop, RangeBounds};

#[derive(Query, State)]
pub struct Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    multiplier: Ratio,
    total: Amount,
    symbol: PhantomData<S>,
    map: Map<K, UnsafeCell<Entry<V, S>>>,
}

impl<K, V, S> Balance for Pool<K, V, S>
where
    K: Terminated + Encode,
    V: State,
    S: Symbol,
{
    fn balance(&self) -> Amount {
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
        todo!();
        // self.multiplier = self.multiplier * multiplier;
        // self.total = (self.total * multiplier)?;

        // Ok(())
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
    V: State + Adjust + Balance,
    V::Encoding: Default,
{
    #[cfg_attr(test, mutate)]
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        todo!();
        // let mut child = self.map.entry(key)?.or_default()?;
        // let mut entry = child.get_mut();
        // if *entry.last_multiplier.numer() == 0 {
        //     entry.last_multiplier = self.multiplier;
        // }

        // if entry.last_multiplier != self.multiplier {
        //     let adjustment = self.multiplier / entry.last_multiplier;
        //     entry.inner.adjust(adjustment)?;
        //     entry.last_multiplier = self.multiplier;
        // }
        // let initial_balance = entry.inner.balance();

        // Ok(ChildMut {
        //     parent_total: &mut self.total,
        //     entry: child,
        //     initial_balance,
        //     _symbol: PhantomData,
        // })
    }

    #[cfg_attr(test, mutate)]
    pub fn get(&self, key: K) -> Result<Child<V, S>> {
        todo!();
        // let entry = self.map.get(key)?.unwrap();
        // Child::new(entry, self.multiplier)
    }
}

pub type IterEntry<'a, K, V, S> = Result<(MapRef<'a, K>, Child<'a, V, S>)>;

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Decode + Terminated + Clone + Next,
    V: State + Adjust + Balance,
    V::Encoding: Default,
{
    // #[cfg_attr(test, mutate)]
    // pub fn range<B>(&self, bounds: B) -> Result<impl Iterator<Item = IterEntry<K, V, S>>>
    // where
    //     B: RangeBounds<K>,
    // {
    //     todo!()
    //     // Ok(self.map.range(bounds)?.map(move |entry| {
    //     //     let entry = entry?;
    //     //     let child = Child::new(entry.1, self.multiplier)?;
    //     //     Ok((entry.0, child))
    //     // }))
    // }

    // #[cfg_attr(test, mutate)]
    // pub fn iter(&self) -> Result<impl Iterator<Item = IterEntry<K, V, S>>> {
    //     self.range(..)
    // }
}

impl<K, V, S> Give<S> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    fn add<A: Into<Amount>>(&mut self, amount: A) -> Result<()> {
        todo!();
        // let amount: Amount<S> = amount.into();

        // if self.total > 0.into() {
        //     let increase = Amount::one() + (amount / self.total)?;
        //     self.multiplier = (self.multiplier * increase)?;
        // } else {
        //     self.multiplier = Amount::one();
        // }

        // self.total += amount;

        Ok(())
    }
}

impl<K, V, S> Take<S> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    fn deduct<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount>,
    {
        todo!();
        // let amount = amount.into();
        // let decrease = amount / self.total;
        // self.total = (self.total - amount)?;
        // self.multiplier = (self.multiplier * decrease)?;

        Ok(())
    }
}

pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance + Adjust,
    V::Encoding: Default,
{
    parent_total: &'a mut Amount,
    entry: MapChildMut<'a, K, UnsafeCell<Entry<V, S>>>,
    initial_balance: Amount,
    _symbol: PhantomData<S>,
}

impl<'a, K, V, S> Drop for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Clone,
    V: State + Balance + Adjust,
    V::Encoding: Default,
{
    fn drop(&mut self) {
        use std::cmp::Ordering::*;
        let start_balance = self.initial_balance;
        let end_balance = self.entry.get_mut().balance();
        match end_balance.cmp(&start_balance) {
            Greater => {
                *self.parent_total += (end_balance - start_balance).expect("Overflow");
            }
            Less => {
                let prev_total = *self.parent_total;
                *self.parent_total = (prev_total
                    - (start_balance - end_balance).expect("Overflow"))
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
    V: State + Balance + Adjust,
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
    V: State + Balance + Adjust,
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
        V: State + Balance + Adjust,
        V::Encoding: Default,
    {
        entry: MapRef<'a, UnsafeCell<Entry<V, S>>>,
        _symbol: PhantomData<S>,
    }

    impl<'a, V, S> Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance + Adjust,
        V::Encoding: Default,
    {
        #[cfg_attr(test, mutate)]
        pub fn new(
            entry_ref: MapRef<'a, UnsafeCell<Entry<V, S>>>,
            current_multiplier: Ratio,
        ) -> Result<Self> {
            todo!()
            // let mut entry = unsafe { &mut *entry_ref.get() };

            // if entry.last_multiplier != current_multiplier {
            //     let adjustment = current_multiplier / entry.last_multiplier;
            //     entry.inner.adjust(adjustment)?;
            //     entry.last_multiplier = current_multiplier;
            // }

            // Ok(Child {
            //     entry: entry_ref,
            //     _symbol: PhantomData,
            // })
        }
    }

    impl<'a, V, S> Deref for Child<'a, V, S>
    where
        S: Symbol,
        V: State + Balance + Adjust,
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
    use crate::coins::{Address, Coin};
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
        // let store = Store::new(Shared::new(MapStore::new()).into());
        // let enc = (Amount::one().into(), Amount::zero().into(), ());
        // let mut pool = Pool::<Address, Coin<Simp>, Simp>::create(store, enc)?;

        // let alice = [0; 32].into();
        // let bob = [1; 32].into();

        // {
        //     let mut alice_child = pool.get_mut(alice)?;
        //     alice_child.add(10)?;
        // }

        // assert_eq!(pool.balance().value, 10);

        // {
        //     let mut bob_child = pool.get_mut(bob)?;
        //     bob_child.add(2)?;
        // }

        // assert_eq!(pool.balance().value, 12);
        // {
        //     let alice_child = pool.get_mut(alice)?;
        //     assert_eq!(alice_child.balance().value, 10);
        // }

        // pool.add(12)?;

        // {
        //     let alice_child = pool.get(alice)?;
        //     assert_eq!(alice_child.balance().value, 20);
        // }

        // assert_eq!(pool.balance().value, 24);

        // pool.take(6)?.burn();

        // assert_eq!(pool.balance().value, 18);
        // {
        //     let alice_child = pool.get_mut(alice)?;
        //     assert_eq!(alice_child.balance().value, 15);
        // }

        // {
        //     let mut alice_child = pool.get_mut(alice)?;
        //     alice_child.take(4)?.burn();
        //     assert_eq!(alice_child.balance().value, 11);
        // }

        // assert_eq!(pool.balance().value, 14);

        // {
        //     let bob_child = pool.get_mut(bob)?;
        //     assert_eq!(bob_child.balance().value, 3);
        // }

        // pool.adjust(Amount::units(2))?;
        // assert_eq!(pool.balance().value, 28);

        // {
        //     let bob_child = pool.get(bob)?;
        //     assert_eq!(bob_child.balance().value, 6);
        // }

        Ok(())
    }
}
