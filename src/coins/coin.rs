//! Tokens with integer amounts.
use super::{Amount, Balance, Decimal, Give, Symbol, Take};
use crate::context::GetContext;
use crate::orga;
use crate::plugins::Paid;
use crate::{Error, Result};
use std::marker::PhantomData;

/// Represents a coin of a specific symbol type.
///
/// This type aims to prevent accidental creation of coins by encouraging
/// explicit minting, and making it more difficult to add amounts of different
/// symbols.
#[orga]
#[derive(Debug)]
pub struct Coin<S: Symbol> {
    /// The amount of the coin.
    pub amount: Amount,
    /// Phantom data to hold the symbol type.
    symbol: PhantomData<S>,
}

impl<S: Symbol> Coin<S> {
    /// Creates a new [Coin] with zero amount.
    pub fn new() -> Self {
        Coin {
            amount: 0.into(),
            symbol: PhantomData,
        }
    }

    /// Creates a new `Coin` with the specified amount.
    pub fn mint<A>(amount: A) -> Self
    where
        A: Into<Amount>,
    {
        Coin {
            amount: amount.into(),
            symbol: PhantomData,
        }
    }

    /// Transfers the coin to the given destination.
    pub fn transfer<G: Give<Coin<S>>>(self, dest: &mut G) -> Result<()> {
        dest.give(self)
    }

    /// Consume the coin.
    pub fn burn(self) {}

    /// Takes coins from self and transfers them to the [Paid] context as
    /// funding.
    pub fn take_as_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self.take(amount)?;
        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Payable context available".into()))?;

        paid.give::<S, _>(taken_coins.amount)
    }
}

impl<S: Symbol> Balance<S, Amount> for Coin<S> {
    fn balance(&self) -> Result<Amount> {
        Ok(self.amount)
    }
}

impl<S: Symbol> Balance<S, Decimal> for Coin<S> {
    fn balance(&self) -> Result<Decimal> {
        Ok(self.amount.into())
    }
}

impl<S: Symbol> Take<S> for Coin<S> {
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount = amount.into();
        if amount > self.amount {
            return Err(Error::Coins("Insufficient funds".into()));
        }
        self.amount = (self.amount - amount)?;

        Ok(Coin::mint(amount))
    }
}

impl<S: Symbol> Give<Self> for Coin<S> {
    fn give(&mut self, value: Coin<S>) -> Result<()> {
        self.amount = (self.amount + value.amount)?;

        Ok(())
    }
}

impl<S: Symbol> From<Amount> for Coin<S> {
    fn from(amount: Amount) -> Self {
        Self::mint(amount)
    }
}

impl<S: Symbol> From<u64> for Coin<S> {
    fn from(amount: u64) -> Self {
        Self::mint(amount)
    }
}
