use super::{Adjust, Amount, Balance, Give, Ratio, Symbol, Take};
use crate::state::State;
use crate::Result;
use std::marker::PhantomData;

#[must_use = "If these coins are meant to be discarded, explicitly call the `burn` method"]
#[derive(State)]
pub struct Coin<S: Symbol> {
    pub amount: Amount,
    symbol: PhantomData<S>,
}

impl<S: Symbol> Default for Coin<S> {
    fn default() -> Self {
        Self {
            amount: Default::default(),
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> Coin<S> {
    pub fn new() -> Self {
        Coin {
            amount: 0.into(),
            symbol: PhantomData,
        }
    }

    pub fn mint<A>(amount: A) -> Self
    where
        A: Into<Amount>,
    {
        Coin {
            amount: amount.into(),
            symbol: PhantomData,
        }
    }

    pub fn transfer<G: Give<S>>(self, dest: &mut G) -> Result<()> {
        dest.add(self.amount)
    }

    pub fn burn(self) {}
}

impl<S: Symbol> Balance<Amount> for Coin<S> {
    fn balance(&self) -> Amount {
        self.amount
    }
}

impl<S: Symbol> Balance<Ratio> for Coin<S> {
    fn balance(&self) -> Ratio {
        self.amount.into()
    }
}

impl<S: Symbol> Adjust for Coin<S> {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()> {
        self.amount = (self.amount * multiplier)?.amount();

        Ok(())
    }
}

impl<S: Symbol> Take<S> for Coin<S> {
    fn take<A: Into<Amount>>(&mut self, amount: A) -> Result<Self::Value> {
        let amount = amount.into();
        self.amount = (self.amount - amount)?;

        Ok(Coin::mint(amount))
    }
}

impl<S: Symbol> Give<S> for Coin<S> {
    fn add<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount>,
    {
        self.amount = (self.amount + amount.into())?;

        Ok(())
    }
}

impl<S: Symbol> From<Amount> for Coin<S> {
    fn from(amount: Amount) -> Self {
        Self::mint(amount)
    }
}
