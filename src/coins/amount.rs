use crate::{Error, Result};
use orga::orga;
use std::convert::TryFrom;

#[orga(skip(Migrate))]
#[derive(Debug, Clone, Copy, crate::migrate::Migrate)]
#[serde(transparent)]
pub struct Amount {
    pub(crate) value: u64,
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Ord for Amount {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Eq for Amount {}

impl Amount {
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
