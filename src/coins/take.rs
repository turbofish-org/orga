use super::{Amount, Symbol};
use crate::Result;

pub trait Take<S: Symbol, U = Amount>: Sized {
    type Value = Self;

    fn take<A: Into<U>>(&mut self, amount: A) -> Result<Self::Value>;
}

pub trait Deduct<S: Symbol, U = Amount>: Sized {
    fn deduct<A: Into<U>>(&mut self, amount: A) -> Result<()>;
}
