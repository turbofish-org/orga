use super::{Amount, MathResult};
use crate::encoding::{Decode, Encode, Terminated};
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use num_rational::Ratio as NumRatio;
use num_traits::CheckedMul;
use std::convert::{TryFrom, TryInto};
use std::ops::{Add, ControlFlow, Div, FromResidual, Mul, Neg, Sub, Try};
use std::result::Result as StdResult;

pub struct Ratio(pub(crate) NumRatio<u64>);

impl From<u64> for Ratio {
    fn from(value: u64) -> Self {
        Ratio(NumRatio::new(value, 1))
    }
}

impl<I> Mul<I> for Ratio
where
    I: TryInto<Ratio>,
    I::Error: std::error::Error,
{
    type Output = MathResult<Ratio>;

    fn mul(self, other: I) -> Self::Output {
        let other = other.try_into()?;
        let mul_res = self.0.checked_mul(&other.0);

        match mul_res {
            Some(res) => MathResult::Ok(Ratio(res)),
            None => MathResult::Err(Error::Unknown),
        }
    }
}

impl<I> Mul<I> for MathResult<Ratio>
where
    I: TryInto<Ratio>,
    I::Error: std::error::Error,
{
    type Output = MathResult<Ratio>;
    fn mul(self, other: I) -> Self::Output {
        let other = other.try_into()?;

        self? * other
    }
}

impl Ratio {
    pub fn new(numer: u64, denom: u64) -> Result<Self> {
        if denom == 0 {
            return Err(Error::DivideByZero); // TODO: use another variant
        }

        Ok(Self(NumRatio::new(numer, denom)))
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
            numerator: 1,
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

impl TryFrom<Amount> for Ratio {
    type Error = Error;

    fn try_from(value: Amount) -> Result<Self> {
        Ok(Ratio(NumRatio::new(value.0, 1)))
    }
}
