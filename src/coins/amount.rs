use super::Symbol;
use crate::query::Query;
use crate::state::State;
use crate::Result;
use ed::{Decode, Encode};
use failure::bail;
use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Div, Mul, Sub};

const PRECISION: u64 = 1_000_000_000;

#[derive(State, Encode, Decode, Debug)]
pub struct Amount<S: Symbol> {
    pub value: u64,
    symbol: PhantomData<S>,
}

impl<S: Symbol> Query for Amount<S> {
    type Query = ();

    fn query(&self, _: ()) -> Result<()> {
        Ok(())
    }
}

impl<S: Symbol> std::fmt::Display for Amount<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<S: Symbol> Default for Amount<S> {
    fn default() -> Self {
        Self {
            value: 0,
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> Clone for Amount<S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> PartialOrd for Amount<S> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.value.cmp(&other.value))
    }
}

impl<S: Symbol> Ord for Amount<S> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl<S: Symbol> PartialEq for Amount<S> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<S: Symbol> Eq for Amount<S> {}

impl<S: Symbol> Copy for Amount<S> {}

impl<S: Symbol> Amount<S> {
    pub fn new(value: u64) -> Self {
        Amount {
            value,
            symbol: PhantomData,
        }
    }

    pub fn zero() -> Self {
        Self::new(0)
    }

    pub fn one() -> Self {
        Self::new(PRECISION)
    }

    pub fn units(value: u64) -> Self {
        Amount {
            value: value * PRECISION,
            symbol: PhantomData,
        }
    }
}

impl<S: Symbol> From<u64> for Amount<S> {
    fn from(value: u64) -> Self {
        Amount::new(value)
    }
}

impl<S: Symbol> PartialEq<u64> for Amount<S> {
    fn eq(&self, other: &u64) -> bool {
        self.value == *other
    }
}

impl<S: Symbol, I: Into<Self>> Add<I> for Amount<S> {
    type Output = Self;

    fn add(self, other: I) -> Self {
        let other = other.into();
        Amount::new(self.value + other.value)
    }
}

impl<S: Symbol, I: Into<Self>> AddAssign<I> for Amount<S> {
    fn add_assign(&mut self, other: I) {
        let other = other.into();
        *self = *self + other;
    }
}

impl<S: Symbol, I: Into<Self>> Mul<I> for Amount<S> {
    type Output = Result<Self>;

    fn mul(self, other: I) -> Result<Self> {
        let other = other.into();
        let value: u128 = self.value.into();
        let value: u128 = value * other.value as u128;
        let value: u128 = value / PRECISION as u128;
        if value > u64::MAX.into() {
            bail!("Overflow")
        } else {
            Ok(Amount::new(value as u64))
        }
    }
}

impl<S: Symbol, I: Into<Self>> Div<I> for Amount<S> {
    type Output = Result<Self>;

    fn div(self, other: I) -> Result<Self> {
        let other = other.into();
        if other.value == 0 {
            bail!("Cannot divide by zero");
        }

        let value: u128 = self.value.into();
        let value: u128 = value * PRECISION as u128;
        let value = value / other.value as u128;
        Ok(Amount::new(value as u64))
    }
}

impl<S: Symbol, I: Into<Self>> Sub<I> for Amount<S> {
    type Output = Result<Self>;

    fn sub(self, other: I) -> Result<Self> {
        let other = other.into();
        match self.value.checked_sub(other.value) {
            Some(value) => Ok(Amount::new(value)),
            None => bail!("Overflow"),
        }
    }
}
