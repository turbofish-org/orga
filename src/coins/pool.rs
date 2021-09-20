use failure::format_err;

use super::{Amount, Give, Symbol, Take};
use crate::collections::map::Ref;
use crate::collections::Map;
use crate::encoding::{Encode, Terminated};
use crate::state::State;
use crate::store::Store;
use crate::Result;
use std::marker::PhantomData;

pub struct Pool<K, V, S>
where
    S: Symbol,
{
    rate: Amount<S>,
    shares: Amount<S>,
    map: Map<K, V>,
}

impl<K, V, S> State for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    #[allow(clippy::type_complexity)]
    type Encoding = (
        <Amount<S> as State>::Encoding,
        <Amount<S> as State>::Encoding,
        <Map<K, V> as State>::Encoding,
    );

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            rate: Amount::create(store.sub(&[0]), data.0)?,
            shares: Amount::create(store.sub(&[1]), data.1)?,
            map: Map::<K, V>::create(store.sub(&[2]), data.2)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((
            <Amount<S> as State>::flush(self.rate)?,
            <Amount<S> as State>::flush(self.shares)?,
            self.map.flush()?,
        ))
    }
}

impl<K, V, S> From<Pool<K, V, S>> for <Pool<K, V, S> as State>::Encoding
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    fn from(pool: Pool<K, V, S>) -> Self {
        (pool.rate, pool.shares, pool.map.into())
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Give<S> + Take<S>,
{
    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        // TODO: check if already exists (should we error if it does, or return the
        // existing value?)
        self.shares += (value.amount()? / self.rate)?;
        self.map.insert(key, value.into())
    }

    pub fn get_mut(&mut self, key: K) -> Result<Option<ChildMut<K, V, S>>> {
        match self.map.contains_key(key.clone())? {
            true => Ok(Some(ChildMut {
                parent: self,
                key,
                _symbol: PhantomData,
            })),
            false => Ok(None),
        }
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State,
{
    pub fn total_value(&self) -> Amount<S> {
        (self.rate * self.shares).expect("Overflow")
    }

    pub fn get(&self, key: K) -> Result<Option<Ref<V>>> {
        self.map.get(key)
    }
}

impl<K, V, S> Give<S> for Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State,
{
    fn add<A: Into<Amount<S>>>(&mut self, amount: A) -> Result<()> {
        let amount = amount.into();
        let total = (self.shares * self.rate).expect("Overflow");
        match amount / total {
            Ok(added_rate) => {
                self.rate = (self.rate * (Amount::one() + added_rate)).expect("Overflow")
            }
            Err(_) => panic!("Cannot pay to empty pool"),
        };

        Ok(())
    }
}

pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State + Take<S> + Give<S>,
{
    parent: &'a mut Pool<K, V, S>,
    key: K,
    _symbol: PhantomData<S>,
}

// impl<'a, K, B, V, S> Deref for ChildMut<'a, K, B, V, S>
// where
//   K: Hash + Eq,
//   B: Borrow<K>,
//   V: Take<S> + Give<S>,
//   S: Symbol,
// {
//   type Target = V;

//   fn deref(&self) -> &'a V {
//     self.parent.map.get(self.key.borrow()).unwrap()
//   }
// }

impl<'a, K, V, S> Give<S> for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Take<S> + Give<S>,
{
    fn add<A: Into<Amount<S>>>(&mut self, amount: A) -> Result<()> {
        let amount = amount.into();
        let shares = (amount / self.parent.rate).expect("Cannot divide by zero");
        self.parent.shares += shares;
        self.parent
            .map
            .get_mut(self.key.clone())?
            .ok_or_else(|| format_err!("Could not add to pool child"))?
            .add(shares)
    }
}

impl<'a, K, V, S> Take<S> for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Take<S> + Give<S>,
{
    fn deduct<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount<S>>,
    {
        let amount = amount.into();
        let shares = (amount / self.parent.rate).expect("Cannot divide by zero");
        self.parent.shares =
            (self.parent.shares - shares).expect("Cannot pay more than is available");
        Ok(())
    }

    fn amount(&self) -> Result<Amount<S>> {
        let amount = self
            .parent
            .map
            .get(self.key.clone())?
            .ok_or_else(|| failure::format_err!("Failed to take amount from child"))?
            .amount()?;
        Ok((amount * self.parent.rate).expect("Overflow"))
    }
}
