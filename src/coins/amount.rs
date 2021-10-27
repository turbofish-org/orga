#[cfg(test)]
use mutagen::mutate;

use super::Ratio;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use ed::{Decode, Encode};
use std::convert::TryInto;
use std::ops::{Add, AddAssign, Div, Mul, Sub};

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

impl PartialOrd for Amount {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

impl Ord for Amount {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Eq for Amount {}

impl Amount {
    #[cfg_attr(test, mutate)]
    pub fn new(value: u64) -> Self {
        Amount(value)
    }
}

impl From<u64> for Amount {
    fn from(value: u64) -> Self {
        Amount::new(value)
    }
}

impl<I: Into<Amount> + Copy> PartialEq<I> for Amount {
    fn eq(&self, other: &I) -> bool {
        self.0 == (*other).into().0
    }
}

impl<I: Into<Self>> Add<I> for Amount {
    type Output = Self;

    fn add(self, other: I) -> Self {
        let other = other.into();
        Amount::new(self.0 + other.0)
    }
}

impl<I: Into<Self>> AddAssign<I> for Amount {
    fn add_assign(&mut self, other: I) {
        let other = other.into();
        *self = *self + other;
    }
}

impl<I: Into<Self>> Div<I> for Amount {
    type Output = Result<Self>;

    fn div(self, other: I) -> Result<Self> {
        todo!()
    }
}

impl<I: Into<Amount>> Sub<I> for Amount {
    type Output = Result<Amount>;

    fn sub(self, other: I) -> Result<Self> {
        todo!()
    }
}

impl<I: TryInto<Ratio>> Mul<I> for Amount {
    type Output = Result<Ratio>;

    fn mul(self, other: I) -> Self::Output {
        todo!()
        // let other: Ratio = other.try_into().map_err(|_| Error::Unknown)?;

        // other * self
    }
}

impl From<Ratio> for Amount {
    fn from(value: Ratio) -> Self {
        Amount::new(value.0.to_integer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops() -> Result<()> {
        let v: Amount = 2.try_into().unwrap();
        let w: Amount = 3.into();

        let x = Ratio::new(3, 1)?;
        let y = Ratio::new(4, 1)?;
        let z = Ratio::new(2, 1)?;

        let a = (x * y * z)?;
        assert_eq!(*a.0.numer(), 24);
        Ok(())
    }
}
