use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use ed::{Decode, Encode};
use std::convert::TryFrom;

#[derive(State, Encode, Decode, Debug, Default, Clone, Copy)]
pub struct Amount(pub(crate) u64);

impl Query for Amount {
    type Query = ();

    fn query(&self, _: ()) -> Result<()> {
        Ok(())
    }
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
        Amount(value)
    }
}

impl From<u64> for Amount {
    fn from(value: u64) -> Self {
        Amount::new(value)
    }
}

impl TryFrom<Result<Amount>> for Amount {
    type Error = Error;

    fn try_from(value: Result<Amount>) -> Result<Self> {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Amount, Ratio};
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn ops() -> Result<()> {
        let v: Amount = 2.try_into().unwrap();
        let w: Amount = 3.into();
        let b = v * w;

        let x = Ratio::new(3, 1)?;
        let y = Ratio::new(4, 1)?;
        let z = Ratio::new(2, 1)?;

        let a = (b * x * y * z)?;
        assert_eq!(*a.0.numer(), 144);
        Ok(())
    }
}
