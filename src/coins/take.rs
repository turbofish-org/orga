use super::{Amount, Coin, Symbol};
use crate::Result;

pub trait Take<S: Symbol> {
    fn deduct<A>(&mut self, amount: A) -> Result<()>
    where
        A: Into<Amount>;

    fn take<A>(&mut self, amount: A) -> Result<Coin<S>>
    where
        A: Into<Amount>,
    {
        let amount = amount.into();
        self.deduct(amount)?;
        Ok(Coin::mint(amount))
    }
}
