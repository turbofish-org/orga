use super::super::{Amount, Decimal, MathResult};
use crate::Error;
use std::ops::Mul;

// Amount * amount
impl Mul<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn mul(self, other: Amount) -> Self::Output {
        MathResult::Ok(
            self.value
                .checked_mul(other.value)
                .ok_or(Error::Overflow)?
                .into(),
        )
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

// Decimal * decimal

impl Mul<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn mul(self, other: Decimal) -> Self::Output {
        self.value
            .checked_mul(other.value)
            .map(|value| value.into())
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
            .value
            .checked_mul(other.value)
            .map(|value| value.into())
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
            .value
            .checked_mul(self.value)
            .map(|value| value.into())
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
