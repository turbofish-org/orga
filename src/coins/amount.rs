//! Safe integer amounts.
use crate::{Error, Result};
use orga::orga;
use std::convert::TryFrom;

/// Represents an amount (usually of coins) with safe arithmetic operations to
/// prevent overflows.
#[orga]
#[derive(Debug, Clone, Copy, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Amount {
    /// The value of the amount.
    pub(crate) value: u64,
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Eq for Amount {}

impl Amount {
    /// Creates a new amount with the given value.
    pub fn new(value: u64) -> Self {
        Amount { value }
    }
}

impl From<u64> for Amount {
    fn from(value: u64) -> Self {
        Amount::new(value)
    }
}

impl From<Amount> for u64 {
    fn from(amount: Amount) -> Self {
        amount.value
    }
}

impl TryFrom<Result<Amount>> for Amount {
    type Error = Error;

    fn try_from(value: Result<Amount>) -> Result<Self> {
        value
    }
}
