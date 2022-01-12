use super::super::{Amount, Decimal, MathResult, Ratio};
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

// Decimal * decimal

impl Mul<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Decimal) -> Self::Output {
        self.0
            .checked_mul(other.0)
            .map(Self)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Mul<Decimal> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Decimal) -> Self::Output {
        self? * other
    }
}

impl Mul<MathResult<Decimal>> for Decimal {
    type Output = MathResult<Decimal>;

    fn mul(self, other: MathResult<Decimal>) -> Self::Output {
        self * other?
    }
}

impl Mul<MathResult<Decimal>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn mul(self, other: MathResult<Decimal>) -> Self::Output {
        self? * other?
    }
}

// Amount * decimal

impl Mul<Decimal> for Amount {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Decimal) -> Self::Output {
        let self_decimal: Decimal = self.into();

        self_decimal
            .0
            .checked_mul(other.0)
            .map(Decimal)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Mul<Decimal> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Decimal) -> Self::Output {
        self? * other
    }
}

impl Mul<MathResult<Decimal>> for Amount {
    type Output = MathResult<Decimal>;

    fn mul(self, other: MathResult<Decimal>) -> Self::Output {
        self * other?
    }
}

impl Mul<MathResult<Decimal>> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn mul(self, other: MathResult<Decimal>) -> Self::Output {
        self? * other?
    }
}

// Amount * decimal

impl Mul<Amount> for Decimal {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Amount) -> Self::Output {
        let other_decimal: Decimal = other.into();

        other_decimal
            .0
            .checked_mul(self.0)
            .map(Decimal)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Mul<Amount> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Amount) -> Self::Output {
        self? * other
    }
}

impl Mul<MathResult<Amount>> for Decimal {
    type Output = MathResult<Decimal>;

    fn mul(self, other: MathResult<Amount>) -> Self::Output {
        self * other?
    }
}

impl Mul<MathResult<Amount>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn mul(self, other: MathResult<Amount>) -> Self::Output {
        self? * other?
    }
}
