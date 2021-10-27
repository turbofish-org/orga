use super::{Amount, Coin, Symbol};
use crate::Result;

pub trait Give<S: Symbol>: Sized {
    fn add<A: Into<Amount>>(&mut self, amount: A) -> Result<()>;

    fn give(&mut self, coin: Coin<S>) -> Result<()> {
        coin.transfer(self)
    }
}
