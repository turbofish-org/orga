use super::{Amount, Give, Symbol, Take};
use crate::Result;

#[must_use = "If these coins are meant to be discarded, explicitly call the `burn` method"]
#[derive(Default)]
pub struct Coin<S: Symbol> {
    pub amount: Amount<S>,
}

impl<S: Symbol> Coin<S> {
    pub fn new() -> Self {
        Coin { amount: 0.into() }
    }

    pub fn mint<A>(amount: A, _symbol: S) -> Self
    where
        A: Into<Amount<S>>,
    {
        Coin {
            amount: amount.into(),
        }
    }

    pub fn transfer<G: Give<S>>(self, dest: &mut G) -> G::Res {
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

    fn amount(&self) -> Amount<S> {
        self.amount
    }
}

impl<S: Symbol> Give<S> for Coin<S> {
    type Res = ();
    fn add<A>(&mut self, amount: A)
    where
        A: Into<Amount<S>>,
    {
        let amount = amount.into();
        self.amount = self.amount + amount;
    }
}
