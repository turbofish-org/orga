use std::marker::PhantomData;

#[cfg(test)]
use mutagen::mutate;

use super::{Adjust, Amount, Balance, Give, Ratio, Symbol, Take};
use crate::state::State;
use crate::Result;

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
    #[cfg_attr(test, mutate)]
    pub fn new() -> Self {
        Coin {
            amount: 0.into(),
            symbol: PhantomData,
        }
    }

    #[cfg_attr(test, mutate)]
    pub fn mint<A>(amount: A) -> Self
    where
        A: Into<Amount>,
    {
        Coin {
            amount: amount.into(),
            symbol: PhantomData,
        }
    }

    #[cfg_attr(test, mutate)]
    pub fn transfer<G: Give<S>>(self, dest: &mut G) -> Result<()> {
        dest.add(self.amount)
    }

    #[cfg_attr(test, mutate)]
    pub fn burn(self) {}
}

impl<S: Symbol> Balance for Coin<S> {
    fn balance(&self) -> Amount {
        self.amount
    }
}

impl<S: Symbol> Take<S> for Coin<S> {
    fn deduct<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount>,
    {
        todo!();
        // let amount = amount.into();
        // self.amount = (self.amount - amount)?;
        Ok(())
    }
}

impl<S: Symbol> Give<S> for Coin<S> {
    fn add<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount>,
    {
        todo!();
        // let amount = amount.into();
        // self.amount += amount;

        Ok(())
    }
}

impl<S: Symbol> Adjust for Coin<S> {
    fn adjust(&mut self, amount: Ratio) -> Result<()> {
        todo!();
        // self.amount = (self.amount * amount)?;
        Ok(())
    }
}
