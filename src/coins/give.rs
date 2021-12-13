use super::{Coin, Symbol};
use crate::Result;

pub trait Give<S: Symbol, V = Coin<S>>: Sized {
    fn give(&mut self, value: V) -> Result<()>;
    fn add<A: Into<V>>(&mut self, amount: A) -> Result<()> {
        self.give(amount.into())
    }
}
