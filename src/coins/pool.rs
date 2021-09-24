use super::{Amount, Give, Symbol, Take};
use crate::collections::map::Ref;
use crate::collections::Map;
use crate::encoding::{Encode, Terminated};
use crate::state::State;
use crate::store::Store;
use crate::Result;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub struct Pool<K, V, S>
where
    S: Symbol,
{
    coins_per_share: Amount<S>,
    shares: Amount<S>,
    map: Map<K, Child<V, S>>,
}

pub struct Child<T, S: Symbol> {
    ps_per_share: Amount<S>,
    pub value: T,
}

impl<T, S: Symbol> Child<T, S> {
    fn new(value: T) -> Self {
        Child {
            ps_per_share: Amount::one(),
            value,
        }
    }
}

impl<T: State, S: Symbol> State for Child<T, S> {
    type Encoding = (<Amount<S> as State>::Encoding, T::Encoding);

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            ps_per_share: data.0,
            value: T::create(store, data.1)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.ps_per_share, self.value.flush()?))
    }
}

impl<T: State, S: Symbol> From<Child<T, S>> for <Child<T, S> as State>::Encoding {
    fn from(child: Child<T, S>) -> Self {
        (child.ps_per_share, child.value.into())
    }
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
            coins_per_share: Amount::create(store.sub(&[0]), data.0)?,
            shares: Amount::create(store.sub(&[1]), data.1)?,
            map: Map::create(store.sub(&[2]), data.2)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((
            <Amount<S> as State>::flush(self.coins_per_share)?,
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
        (pool.coins_per_share, pool.shares, pool.map.into())
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Give<S> + Take<S>,
    V::Encoding: Default,
{
    pub fn get_mut(&mut self, key: K) -> Result<ChildMut<K, V, S>> {
        let mut child = self.map.entry(key.clone())?.or_insert_default()?;

        let child_shares = child.value.amount()?;
        let coins = ((child_shares * child.ps_per_share)? * self.coins_per_share)?;
        child.ps_per_share = (Amount::one() / self.coins_per_share)?;

        use std::cmp::Ordering::*;
        match coins.cmp(&child_shares) {
            Greater => {
                let adjustment = (coins - child_shares)?;
                child.value.add(adjustment)?;
            }
            Less => {
                let adjustment = (child_shares - coins)?;
                child.value.deduct(adjustment)?;
            }
            Equal => {}
        }

        Ok(ChildMut {
            parent: self,
            key,
            _symbol: PhantomData,
        })
    }
}

impl<K, V, S> Pool<K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State,
    V::Encoding: Default,
{
    pub fn total_value(&self) -> Amount<S> {
        (self.coins_per_share * self.shares).expect("Overflow")
    }

    pub fn get(&self, key: K) -> Result<Ref<Child<V, S>>> {
        self.map.get_or_default(key)
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
        let total = (self.shares * self.coins_per_share).expect("Overflow");
        match amount / total {
            Ok(added_rate) => {
                self.coins_per_share =
                    (self.coins_per_share * (Amount::one() + added_rate)).expect("Overflow")
            }
            Err(_) => panic!("Cannot pay to empty pool"),
        };

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
        todo!()
    }

    fn amount(&self) -> Result<Amount<S>> {
        todo!()
    }
}

pub struct ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated,
    V: State + Give<S>,
{
    parent: &'a mut Pool<K, V, S>,
    key: K,
    _symbol: PhantomData<S>,
}

impl<'a, K, V, S> Deref for ChildMut<'a, K, V, S>
where
    K: Encode + Terminated,
    V: State + Give<S>,
    S: Symbol,
{
    type Target = V;

    fn deref(&self) -> &'a V {
        todo!()
    }
}

impl<'a, K, V, S> DerefMut for ChildMut<'a, K, V, S>
where
    K: Encode + Terminated,
    V: State + Give<S>,
    S: Symbol,
{
    fn deref_mut(&mut self) -> &'a mut V {
        todo!()
    }
}

impl<'a, K, V, S> Give<S> for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Give<S>,
    V::Encoding: Default,
{
    fn add<A: Into<Amount<S>>>(&mut self, amount: A) -> Result<()> {
        let amount = amount.into();

        let pool_shares = (amount / self.parent.coins_per_share).expect("Cannot divide by zero");
        self.parent.shares += pool_shares;

        let mut child = self
            .parent
            .map
            .entry(self.key.clone())?
            .or_insert_default()?;
        let child_shares = (pool_shares / child.ps_per_share).expect("Cannot divide by zero");
        child.value.add(child_shares)
    }
}

impl<'a, K, V, S> Take<S> for ChildMut<'a, K, V, S>
where
    S: Symbol,
    K: Encode + Terminated + Clone,
    V: State + Take<S> + Give<S>,
    V::Encoding: Default,
{
    fn deduct<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount<S>>,
    {
        let amount = amount.into();

        let pool_shares = (amount / self.parent.coins_per_share)?;
        self.parent.shares = (self.parent.shares - pool_shares)?;

        let mut child = self
            .parent
            .map
            .entry(self.key.clone())?
            .or_insert_default()?;
        let child_shares = (amount / child.ps_per_share)?;

        child.value.deduct(child_shares)
    }

    fn amount(&self) -> Result<Amount<S>> {
        let child = self.parent.map.get_or_default(self.key.clone())?;
        let pool_shares = (child.value.amount()? * child.ps_per_share)?;
        pool_shares * self.parent.coins_per_share
    }
}
