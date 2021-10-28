use super::super::{Amount, MathResult, Ratio};
use crate::Error;
use num_traits::CheckedMul;
use std::ops::Mul;
use MathResult::*;

// Amount * amount
impl Mul<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn mul(self, other: Amount) -> Self::Output {
        MathResult::Ok(self.0.checked_mul(other.0).ok_or(Error::Overflow)?.into())
    }
}

impl Mul<Amount> for MathResult<Amount> {
    type Output = MathResult<Amount>;

    fn mul(self, other: Amount) -> Self::Output {
        self? * other
    }
}

impl Mul<MathResult<Amount>> for Amount {
    type Output = MathResult<Amount>;

    fn mul(self, other: MathResult<Amount>) -> Self::Output {
        self * other?
    }
}

impl Mul<MathResult<Amount>> for MathResult<Amount> {
    type Output = MathResult<Amount>;

    fn mul(self, other: MathResult<Amount>) -> Self::Output {
        self? * other?
    }
}

// Amount * ratio

impl Mul<Ratio> for Amount {
    type Output = MathResult<Ratio>;

    fn mul(self, other: Ratio) -> Self::Output {
        Ok(other
            .0
            .checked_mul(&self.0.into())
            .ok_or(Error::Overflow)?
            .into())
    }
}

impl Mul<Ratio> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn mul(self, other: Ratio) -> Self::Output {
        self? * other
    }
}

impl Mul<MathResult<Ratio>> for Amount {
    type Output = MathResult<Ratio>;

    fn mul(self, other: MathResult<Ratio>) -> Self::Output {
        self * other?
    }
}

impl Mul<MathResult<Ratio>> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn mul(self, other: MathResult<Ratio>) -> Self::Output {
        self? * other?
    }
}

// Ratio * ratio

impl Mul<Ratio> for Ratio {
    type Output = MathResult<Ratio>;

    fn mul(self, other: Ratio) -> Self::Output {
        Ok(other.0.checked_mul(&self.0).ok_or(Error::Overflow)?.into())
    }
}

impl Mul<Ratio> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn mul(self, other: Ratio) -> Self::Output {
        self? * other
    }
}

impl Mul<MathResult<Ratio>> for Ratio {
    type Output = MathResult<Ratio>;

    fn mul(self, other: MathResult<Ratio>) -> Self::Output {
        self * other?
    }
}

impl Mul<MathResult<Ratio>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn mul(self, other: MathResult<Ratio>) -> Self::Output {
        self? * other?
    }
}

// Ratio * amount

impl Mul<Amount> for Ratio {
    type Output = MathResult<Ratio>;

    fn mul(self, other: Amount) -> Self::Output {
        other * self
    }
}

impl Mul<Amount> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn mul(self, other: Amount) -> Self::Output {
        other * self?
    }
}

impl Mul<MathResult<Amount>> for Ratio {
    type Output = MathResult<Ratio>;

    fn mul(self, other: MathResult<Amount>) -> Self::Output {
        other? * self
    }
}

impl Mul<MathResult<Amount>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn mul(self, other: MathResult<Amount>) -> Self::Output {
        other? * self?
    }
}
