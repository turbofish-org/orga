//! Types from which value may be taken.
use super::{Amount, Symbol};
use crate::Result;

/// A trait for types from which value may be taken.
pub trait Take<S: Symbol, U = Amount>: Sized {
    /// The value type returned by [Self::take].
    type Value = Self;

    /// Take an amount of value.
    fn take<A: Into<U>>(&mut self, amount: A) -> Result<Self::Value>;
}

/// A trait for types from which value may be destroyed.
pub trait Deduct<S: Symbol, U = Amount>: Sized {
    /// Destroy an amount of value.
    fn deduct<A: Into<U>>(&mut self, amount: A) -> Result<()>;
}
