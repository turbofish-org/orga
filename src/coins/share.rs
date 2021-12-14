use super::{Adjust, Amount, Balance, Coin, Decimal, Give, Symbol, Take};
use crate::state::State;
use crate::{Error, Result};
use std::marker::PhantomData;

#[derive(State, Debug)]
pub struct Share<S: Symbol> {
    pub shares: Decimal,
    symbol: PhantomData<S>,
}

impl<S: Symbol> Default for Share<S> {
    fn default() -> Self {
        Self {
            shares: Default::default(),
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> Share<S> {
    pub fn new() -> Self {
        Share {
            shares: 0.into(),
            symbol: PhantomData,
        }
    }

    pub fn amount(&self) -> Result<Amount> {
        self.shares.amount()
    }
}

impl<S: Symbol> Balance<S, Amount> for Share<S> {
    fn balance(&self) -> Result<Amount> {
        self.shares.amount()
    }
}

impl<S: Symbol> Balance<S, Decimal> for Share<S> {
    fn balance(&self) -> Result<Decimal> {
        Ok(self.shares)
    }
}

impl<S: Symbol> Adjust for Share<S> {
    fn adjust(&mut self, multiplier: Decimal) -> Result<()> {
        self.shares = (self.shares * multiplier)?;

        Ok(())
    }
}

impl<S: Symbol> Take<S, Amount> for Share<S> {
    type Value = Coin<S>;
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount: Amount = amount.into();
        if self.shares < amount {
            return Err(Error::Coins("Insufficient balance".into()));
        }
        self.shares = (self.shares - amount)?;

        Ok(Coin::mint(amount))
    }
}

impl<S: Symbol> Give<S> for Share<S> {
    fn give(&mut self, coin: Coin<S>) -> Result<()> {
        self.shares = (self.shares + coin.amount)?;

        Ok(())
    }
}

impl<S: Symbol> From<Decimal> for Share<S> {
    fn from(amount: Decimal) -> Self {
        Self {
            shares: amount,
            ..Default::default()
        }
    }
}

impl<S: Symbol> From<Coin<S>> for Share<S> {
    fn from(coins: Coin<S>) -> Self {
        Self {
            shares: coins.amount.into(),
            ..Default::default()
        }
    }
}
