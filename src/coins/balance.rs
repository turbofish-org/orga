//! Access the balance of value-holding types.

use super::{Amount, Symbol};
use crate::Result;

/// Provides a unified way to access the balance of various coin-holding
/// values. It can be implemented for different underlying units, e.g. `Amount`
/// or `Decimal`.
pub trait Balance<S: Symbol, U = Amount> {
    /// Returns the current balance for the given symbol.
    fn balance(&self) -> Result<U>;
}
