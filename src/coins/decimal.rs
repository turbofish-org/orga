use super::Amount;
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use rust_decimal::prelude::{Decimal as NumDecimal, *};
use std::cmp::Ordering;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub struct Decimal(pub(crate) NumDecimal);

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Encode for Decimal {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        dest.write_all(&self.0.serialize())?;

        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(16)
    }
}

impl Decode for Decimal {
    fn decode<R: std::io::Read>(mut source: R) -> ed::Result<Self> {
        let mut bytes = [0u8; 16];
        source.read_exact(&mut bytes)?;
        Ok(Self(NumDecimal::deserialize(bytes)))
    }
}

impl ed::Terminated for Decimal {}

impl Default for Decimal {
    fn default() -> Self {
        0.into()
    }
}

impl From<u64> for Decimal {
    fn from(value: u64) -> Self {
        Decimal(value.into())
    }
}

impl Eq for Decimal {}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Decimal {
    pub fn amount(&self) -> Result<Amount> {
        if self.0.is_sign_negative() {
            Err(Error::Coins("Amounts may not be negative".into()))
        } else {
            match self.0.round().to_u64() {
                Some(value) => Ok(value.into()),
                None => Err(Error::Coins(
                    "Amounts may not be greater than u64::MAX".into(),
                )),
            }
        }
    }

    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    pub fn zero() -> Self {
        Self(NumDecimal::ZERO)
    }

    pub fn one() -> Self {
        Self(NumDecimal::ONE)
    }
}

#[derive(Encode, Decode)]
pub struct DecimalEncoding(pub(crate) [u8; 16]);

impl Default for DecimalEncoding {
    fn default() -> Self {
        Decimal(0.into()).into()
    }
}

impl State for Decimal {
    type Encoding = DecimalEncoding;
    fn create(_store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self(NumDecimal::deserialize(data.0)))
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(self.into())
    }
}

impl From<Decimal> for DecimalEncoding {
    fn from(decimal: Decimal) -> Self {
        DecimalEncoding(decimal.0.serialize())
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
        Decimal(value)
    }
}

impl From<Amount> for Decimal {
    fn from(amount: Amount) -> Self {
        Self(amount.0.into())
    }
}

impl FromStr for Decimal {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self(NumDecimal::from_str(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::Decimal;

    #[test]
    fn format() {
        let formatted: Decimal = rust_decimal_macros::dec!(1.23).into();

        assert_eq!(format!("{}", formatted), "1.23");
    }
}
