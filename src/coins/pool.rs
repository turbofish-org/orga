use super::{Adjust, Amount, Balance, Give, Symbol, Take};
use crate::collections::map::{ChildMut as MapChildMut, Ref as MapRef};
use crate::collections::Map;
use crate::encoding::{Encode, Terminated};
use crate::state::State;
use crate::store::Store;
use crate::Result;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Drop};

pub struct Pool<K, V, S>
where
    S: Symbol,
{
    multiplier: Amount<S>,
    total: Amount<S>,
    map: Map<K, Entry<V, S>>,
}

impl<K, V, S> Balance<S> for Pool<K, V, S>
where
    S: Symbol,
{
    fn balance(&self) -> Amount<S> {
        self.total
    }
}

pub struct Entry<T, S: Symbol> {
    last_multiplier: Amount<S>,
    inner: T,
}

impl<T, S: Symbol> Deref for Entry<T, S> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, S: Symbol> DerefMut for Entry<T, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: State, S: Symbol> State for Entry<T, S> {
    type Encoding = (<Amount<S> as State>::Encoding, T::Encoding);

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            last_multiplier: data.0,
            inner: T::create(store, data.1)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.last_multiplier, self.inner.flush()?))
    }
}

impl<T: State, S: Symbol> From<Entry<T, S>> for <Entry<T, S> as State>::Encoding {
    fn from(child: Entry<T, S>) -> Self {
        (child.last_multiplier, child.inner.into())
    }
}

impl<K, V, S> State for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State + Balance<S> + Adjust<S>,
{
    #[allow(clippy::type_complexity)]
    type Encoding = (
        <Amount<S> as State>::Encoding,
        <Amount<S> as State>::Encoding,
        <Map<K, V> as State>::Encoding,
    );

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            multiplier: Amount::create(store.sub(&[0]), data.0)?,
            total: Amount::create(store.sub(&[1]), data.1)?,
            map: Map::create(store.sub(&[2]), data.2)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((
            <Amount<S> as State>::flush(self.multiplier)?,
            <Amount<S> as State>::flush(self.total)?,
            self.map.flush()?,
        ))
    }
}

impl<K, V, S> From<Pool<K, V, S>> for <Pool<K, V, S> as State>::Encoding
where
    S: Symbol,
    K: Encode + Terminated,
    V: State + Adjust<S> + Balance<S>,
{
    fn from(pool: Pool<K, V, S>) -> Self {
        (pool.multiplier, pool.total, pool.map.into())
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Adjust<S> + Balance<S>,
    V::Encoding: Default,
{
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        let mut entry = self.map.entry(key)?.or_default()?;

        if entry.last_multiplier == Amount::zero() {
            entry.last_multiplier = self.multiplier;
        }

        if entry.last_multiplier != self.multiplier {
            let adjustment = (self.multiplier / entry.last_multiplier)?;
            entry.inner.adjust(adjustment)?;
            entry.last_multiplier = self.multiplier;
        }
        let initial_balance = entry.inner.balance();

        Ok(ChildMut {
            parent_total: &mut self.total,
            entry,
            initial_balance,
            _symbol: PhantomData,
        })
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Balance<S> + Adjust<S>,
    V::Encoding: Default,
{
}

impl<K, V, S> Give<S> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    fn add<A: Into<Amount<S>>>(&mut self, amount: A) -> Result<()> {
        let amount: Amount<S> = amount.into();

        if self.total > 0.into() {
            let increase = Amount::one() + (amount / self.total)?;
            self.multiplier = (self.multiplier * increase)?;
        } else {
            self.multiplier = Amount::one();
        }

        self.total += amount;

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
        A: Into<Amount<S>>,
    {
        let amount = amount.into();
        let decrease = (Amount::one() - (amount / self.total)?)?;
        self.total = (self.total - amount)?;
        self.multiplier = (self.multiplier * decrease)?;

        Ok(())
    }
}

pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Balance<S> + Adjust<S>,
    V::Encoding: Default,
{
    parent_total: &'a mut Amount<S>,
    entry: MapChildMut<'a, K, Entry<V, S>>,
    initial_balance: Amount<S>,
    _symbol: PhantomData<S>,
}

impl<'a, K, V, S> Drop for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Balance<S> + Adjust<S>,
    V::Encoding: Default,
{
    fn drop(&mut self) {
        use std::cmp::Ordering::*;
        let start_balance = self.initial_balance;
        let end_balance = self.entry.inner.balance();
        match end_balance.value.cmp(&start_balance.value) {
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
    K: Encode + Terminated + Clone,
    V: State + Balance<S> + Adjust<S>,
    V::Encoding: Default,
{
    type Target = MapChildMut<'a, K, Entry<V, S>>;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl<'a, K, V, S> DerefMut for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Balance<S> + Adjust<S>,
    V::Encoding: Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coins::{Address, Coin};
    use crate::encoding::{Decode, Encode};
    use crate::store::{MapStore, Shared, Store};

    #[derive(Encode, Decode, Debug)]
    struct Simp;
    impl Symbol for Simp {}

    #[test]
    fn simple_pool() -> Result<()> {
        let store = Store::new(Shared::new(MapStore::new()).into());
        let enc = (Amount::one(), Amount::zero(), ());
        let mut pool = Pool::<Address, Coin<Simp>, Simp>::create(store, enc)?;

        let alice = [0; 32].into();
        let bob = [1; 32].into();

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.add(10)?;
        }

        assert_eq!(pool.balance().value, 10);

        {
            let mut bob_child = pool.get_mut(bob)?;
            bob_child.add(2)?;
        }

        assert_eq!(pool.balance().value, 12);
        {
            let alice_child = pool.get_mut(alice)?;
            assert_eq!(alice_child.balance().value, 10);
        }

        pool.add(12)?;
        {
            let alice_child = pool.get_mut(alice)?;
            assert_eq!(alice_child.balance().value, 20);
        }

        assert_eq!(pool.balance().value, 24);

        pool.take(6)?.burn();

        assert_eq!(pool.balance().value, 18);
        {
            let alice_child = pool.get_mut(alice)?;
            assert_eq!(alice_child.balance().value, 15);
        }

        {
            let mut alice_child = pool.get_mut(alice)?;
            alice_child.take(4)?.burn();
            assert_eq!(alice_child.balance().value, 11);
        }

        assert_eq!(pool.balance().value, 14);

        {
            let bob_child = pool.get_mut(bob)?;
            assert_eq!(bob_child.balance().value, 3);
        }

        Ok(())
    }
}
