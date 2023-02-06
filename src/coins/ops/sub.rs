use super::super::{Amount, Decimal, MathResult};
use crate::Error;
use std::ops::Sub;

// Amount - amount

impl Sub<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn sub(self, other: Amount) -> Self::Output {
        self.value
            .checked_sub(other.value)
            .map(|value| value.into())
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Amount> for MathResult<Amount> {
    type Output = MathResult<Amount>;

    fn sub(self, other: Amount) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Amount>> for Amount {
    type Output = MathResult<Amount>;

    fn sub(self, other: MathResult<Amount>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Amount>> for MathResult<Amount> {
    type Output = MathResult<Amount>;

    fn sub(self, other: MathResult<Amount>) -> Self::Output {
        self? - other?
    }
}

// Decimal - decimal

impl Sub<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Decimal) -> Self::Output {
        self.value
            .checked_sub(other.value)
            .map(|value| value.into())
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Decimal> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Decimal) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Decimal>> for Decimal {
    type Output = MathResult<Decimal>;

    fn sub(self, other: MathResult<Decimal>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Decimal>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn sub(self, other: MathResult<Decimal>) -> Self::Output {
        self? - other?
    }
}

// Amount - decimal

impl Sub<Decimal> for Amount {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Decimal) -> Self::Output {
        let self_decimal: Decimal = self.into();

        self_decimal
            .value
            .checked_sub(other.value)
            .map(|value| value.into())
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Decimal> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Decimal) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Decimal>> for Amount {
    type Output = MathResult<Decimal>;

    fn sub(self, other: MathResult<Decimal>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Decimal>> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn sub(self, other: MathResult<Decimal>) -> Self::Output {
        self? - other?
    }
}

// Decimal - amount

impl Sub<Amount> for Decimal {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Amount) -> Self::Output {
        let other_decimal: Decimal = other.into();

        self.value
            .checked_sub(other_decimal.value)
            .map(|value| value.into())
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Amount> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Amount) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Amount>> for Decimal {
    type Output = MathResult<Decimal>;

    fn sub(self, other: MathResult<Amount>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Amount>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn sub(self, other: MathResult<Amount>) -> Self::Output {
        self? - other?
    }
}
