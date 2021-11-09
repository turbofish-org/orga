use super::{Amount, Symbol};
use crate::Result;

pub trait Take<S: Symbol, U = Amount>: Sized {
    type Value = Self;

    fn take<A: Into<U>>(&mut self, amount: A) -> Result<Self::Value>;
}

pub trait Deduct<S: Symbol, U = Amount>: Sized {
    fn deduct<A: Into<U>>(&mut self, amount: A) -> Result<()>;
}

// impl<T, S, U> Take<S, U> for T
// where
//     S: Symbol,
//     T: Deduct<S, U> + From<U>,
//     U: Clone,
// {
//     fn take<A: Into<U>>(&mut self, amount: A) -> Result<Self::Value> {
//         let amount = amount.into();
//         self.deduct(amount.clone())?;
//         let value = amount.into();

//         Ok(value)
//     }
// }
