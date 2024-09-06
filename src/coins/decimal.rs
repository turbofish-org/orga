//! Safe decimal amounts.
use super::Amount;
use crate::describe::{Builder, Describe};
use crate::encoding::Adapter;
use crate::encoding::{Decode, Encode};
use crate::migrate::Migrate;
use crate::orga;
use crate::{Error, Result};
use rust_decimal::{prelude::ToPrimitive, Decimal as NumDecimal};

use std::convert::TryFrom;
use std::str::FromStr;

/// A decimal type for precise financial calculations.
#[orga(simple, skip(Describe, Migrate))]
#[derive(Copy, Debug, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Decimal {
    /// The underlying numeric decimal value.
    pub(crate) value: NumDecimal,
}

impl Migrate for Decimal {}

impl Describe for Decimal {
    fn describe() -> crate::describe::Descriptor {
        Builder::new::<Self>().build()
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl Encode for Adapter<Decimal> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        dest.write_all(&self.0.value.serialize())?;

        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(16)
    }
}

impl Decode for Adapter<Decimal> {
    fn decode<R: std::io::Read>(mut source: R) -> ed::Result<Self> {
        let mut bytes = [0u8; 16];
        source.read_exact(&mut bytes)?;
        Ok(Decimal {
            value: NumDecimal::deserialize(bytes),
        }
        .into())
    }
}

impl ed::Terminated for Adapter<Decimal> {}

impl From<u64> for Decimal {
    fn from(value: u64) -> Self {
        Decimal {
            value: value.into(),
        }
    }
}

impl Eq for Decimal {}

impl Decimal {
    /// Converts the decimal to an `Amount`, rounding to the nearest integer.
    /// Returns an error if the value is negative or exceeds u64::MAX.
    pub fn amount(&self) -> Result<Amount> {
        if self.value.is_sign_negative() {
            Err(Error::Coins("Amounts may not be negative".into()))
        } else {
            match self.value.round().to_u64() {
                Some(value) => Ok(value.into()),
                None => Err(Error::Coins(
                    "Amounts may not be greater than u64::MAX".into(),
                )),
            }
        }
    }

    /// Returns the absolute value of the decimal.
    pub fn abs(&self) -> Self {
        Decimal {
            value: self.value.abs(),
        }
    }

    /// Returns a new `Decimal` with value zero.
    pub fn zero() -> Self {
        Decimal {
            value: NumDecimal::ZERO,
        }
    }

    /// Returns a new `Decimal` with value one.
    pub fn one() -> Self {
        Decimal {
            value: NumDecimal::ONE,
        }
    }
}

impl TryFrom<Result<Decimal>> for Decimal {
    type Error = Error;

    fn try_from(value: Result<Decimal>) -> Result<Self> {
        value
    }
}

impl From<NumDecimal> for Decimal {
    fn from(value: NumDecimal) -> Self {
        Decimal { value }
    }
}

impl From<Amount> for Decimal {
    fn from(amount: Amount) -> Self {
        Self {
            value: amount.value.into(),
        }
    }
}

impl FromStr for Decimal {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self {
            value: NumDecimal::from_str(s)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format() {
        let formatted: Decimal = rust_decimal_macros::dec!(1.23).into();
        assert_eq!(format!("{}", formatted), "1.23");
    }
}
