use super::{Amount, Give, Symbol, Take};
use crate::encoding::{Decode, Encode};
use crate::Result;

#[must_use = "If these coins are meant to be discarded, explicitly call the `burn` method"]
#[derive(Encode, Decode)]
pub struct Coin<S: Symbol> {
    pub amount: Amount<S>,
}

impl<S: Symbol> Default for Coin<S> {
    fn default() -> Self {
        Self {
            amount: Default::default(),
        }
    }
}

impl<S: Symbol> Coin<S> {
    pub fn new() -> Self {
        Coin { amount: 0.into() }
    }

    pub fn mint<A>(amount: A) -> Self
    where
        A: Into<Amount<S>>,
    {
        Coin {
            amount: amount.into(),
        }
    }

    pub fn transfer<G: Give<S>>(self, dest: &mut G) -> Result<()> {
        dest.add(self.amount)
    }

    pub fn burn(self) {}
}

impl<S: Symbol> Take<S> for Coin<S> {
    fn deduct<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount<S>>,
    {
        let amount = amount.into();
        self.amount = (self.amount - amount)?;
        Ok(())
    }

    fn amount(&self) -> Result<Amount<S>> {
        Ok(self.amount)
    }
}

impl<S: Symbol> Give<S> for Coin<S> {
    fn add<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount<S>>,
    {
        let amount = amount.into();
        self.amount += amount;

        Ok(())
    }
}
