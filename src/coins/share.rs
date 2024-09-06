//! Tokens with decimal amounts.
use super::{Amount, Balance, Coin, Decimal, Give, Symbol, Take};
use crate::orga;
use crate::{Error, Result};
use std::marker::PhantomData;

/// Represents a share of a specific coin type. Similar to a `[Coin]` but using
/// a `Decimal` to represent the amount.
#[orga]
#[derive(Debug)]
pub struct Share<S: Symbol> {
    /// The amount.
    pub shares: Decimal,
    /// Phantom data to hold the symbol type.
    symbol: PhantomData<S>,
}

impl<S: Symbol> Share<S> {
    /// Creates a new `Share` with zero shares.
    pub fn new() -> Self {
        Share {
            shares: 0.into(),
            symbol: PhantomData,
        }
    }

    /// Converts the share's decimal value to an `Amount`.
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

impl<S: Symbol> Give<Coin<S>> for Share<S> {
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

#[cfg(test)]
impl<S: Symbol> Give<(u8, Amount)> for Share<S> {
    fn give(&mut self, (id, amount): (u8, Amount)) -> Result<()> {
        if id != S::INDEX {
            return Err(Error::Coins("Invalid symbol index".into()));
        }
        self.shares = (self.shares + amount)?;

        Ok(())
    }
}
