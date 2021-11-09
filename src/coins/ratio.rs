use super::Amount;
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use num_rational::Ratio as NumRatio;
use std::cmp::Ordering;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub struct Ratio(pub(crate) NumRatio<u64>);

impl Default for Ratio {
    fn default() -> Self {
        0.into()
    }
}

impl From<u64> for Ratio {
    fn from(value: u64) -> Self {
        Ratio(NumRatio::new(value, 1))
    }
}

impl From<NumRatio<u64>> for Ratio {
    fn from(value: NumRatio<u64>) -> Self {
        Ratio(value)
    }
}

impl Eq for Ratio {}

impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Ratio {
    pub fn new(numer: u64, denom: u64) -> Result<Self> {
        if denom == 0 {
            return Err(Error::DivideByZero); // TODO: use another variant
        }

        Ok(Self(NumRatio::new(numer, denom)))
    }

    pub fn amount(&self) -> Amount {
        Amount::new(self.0.to_integer())
    }
}

#[derive(Encode, Decode)]
pub struct RatioEncoding {
    numerator: u64,
    denominator: u64,
}
impl Default for RatioEncoding {
    fn default() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }
}

impl State for Ratio {
    type Encoding = RatioEncoding;
    fn create(_store: Store, data: Self::Encoding) -> Result<Self> {
        Ratio::new(data.numerator, data.denominator)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(RatioEncoding {
            numerator: *(self.0).numer(),
            denominator: *(self.0).denom(),
        })
    }
}

impl From<Ratio> for RatioEncoding {
    fn from(ratio: Ratio) -> Self {
        RatioEncoding {
            numerator: *(ratio.0).numer(),
            denominator: *(ratio.0).denom(),
        }
    }
}

impl TryFrom<Result<Ratio>> for Ratio {
    type Error = Error;

    fn try_from(value: Result<Ratio>) -> Result<Self> {
        value
    }
}

impl From<Amount> for Ratio {
    fn from(amount: Amount) -> Self {
        Self::new(amount.0, 1).unwrap()
    }
}
