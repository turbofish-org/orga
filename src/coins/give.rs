//! Types which may receive value.
use crate::Result;

/// Trait for adding value to a receiver.
///
/// This trait is typically implemented for types that represent
/// accounts, balances, or other entities that can receive coins or
/// other forms of value.
pub trait Give<V> {
    /// Tries to add the given value to the receiver.
    fn give(&mut self, value: V) -> Result<()>;
}
