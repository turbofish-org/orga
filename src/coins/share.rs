use std::marker::PhantomData;

use super::{Adjust, Amount, Balance, Coin, Give, Ratio, Symbol, Take};
use crate::state::State;
use crate::Result;

#[derive(State, Debug)]
pub struct Share<S: Symbol> {
    pub amount: Ratio,
    symbol: PhantomData<S>,
}

impl<S: Symbol> Default for Share<S> {
    fn default() -> Self {
        Self {
            amount: Default::default(),
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> Share<S> {
    pub fn new() -> Self {
        Share {
            amount: 0.into(),
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> Balance<S, Amount> for Share<S> {
    fn balance(&self) -> Amount {
        self.amount.amount()
    }
}

impl<S: Symbol> Balance<S, Ratio> for Share<S> {
    fn balance(&self) -> Ratio {
        self.amount
    }
}

impl<S: Symbol> Adjust for Share<S> {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        self.amount = (self.amount * multiplier)?;

        Ok(())
    }
}

impl<S: Symbol> Take<S, Amount> for Share<S> {
    type Value = Coin<S>;
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount: Amount = amount.into();
        self.amount = (self.amount - amount)?;

        Ok(Coin::mint(amount))
    }
}

impl<S: Symbol> Give<S> for Share<S> {
    fn give(&mut self, coin: Coin<S>) -> Result<()> {
        self.amount = (self.amount + coin.amount)?;

        Ok(())
    }
}

impl<S: Symbol> From<Ratio> for Share<S> {
    fn from(amount: Ratio) -> Self {
        Self {
            amount,
            ..Default::default()
        }
    }
}

impl<S: Symbol> From<Coin<S>> for Share<S> {
    fn from(coins: Coin<S>) -> Self {
        Self {
            amount: coins.amount.into(),
            ..Default::default()
        }
    }
}
