use super::{Amount, Coin, Symbol};

pub trait Give<S: Symbol>: Sized {
    type Res;

    fn add<A: Into<Amount<S>>>(&mut self, amount: A) -> Self::Res;

    fn give(&mut self, coin: Coin<S>) -> Self::Res {
        coin.transfer(self)
    }
}
