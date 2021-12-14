use super::super::{Amount, Decimal, MathResult, Ratio};
use crate::Error;
use num_traits::CheckedSub;
use std::ops::Sub;

// Amount - amount

impl Sub<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn sub(self, other: Amount) -> Self::Output {
        self.0
            .checked_sub(other.0)
            .map(Self)
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

// Amount - ratio

impl Sub<Ratio> for Amount {
    type Output = MathResult<Ratio>;

    fn sub(self, other: Ratio) -> Self::Output {
        let self_ratio: Ratio = self.into();

        self_ratio
            .0
            .checked_sub(&other.0)
            .map(Ratio)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Ratio> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn sub(self, other: Ratio) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Ratio>> for Amount {
    type Output = MathResult<Ratio>;

    fn sub(self, other: MathResult<Ratio>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Ratio>> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn sub(self, other: MathResult<Ratio>) -> Self::Output {
        self? - other?
    }
}

// Ratio - ratio

impl Sub<Ratio> for Ratio {
    type Output = MathResult<Ratio>;

    fn sub(self, other: Ratio) -> Self::Output {
        self.0
            .checked_sub(&other.0)
            .map(Ratio)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Ratio> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn sub(self, other: Ratio) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Ratio>> for Ratio {
    type Output = MathResult<Ratio>;

    fn sub(self, other: MathResult<Ratio>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Ratio>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn sub(self, other: MathResult<Ratio>) -> Self::Output {
        self? - other?
    }
}

// Ratio - amount

impl Sub<Amount> for Ratio {
    type Output = MathResult<Ratio>;

    fn sub(self, other: Amount) -> Self::Output {
        let other_ratio: Ratio = other.into();

        self.0
            .checked_sub(&other_ratio.0)
            .map(Ratio)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Sub<Amount> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn sub(self, other: Amount) -> Self::Output {
        self? - other
    }
}

impl Sub<MathResult<Amount>> for Ratio {
    type Output = MathResult<Ratio>;

    fn sub(self, other: MathResult<Amount>) -> Self::Output {
        self - other?
    }
}

impl Sub<MathResult<Amount>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn sub(self, other: MathResult<Amount>) -> Self::Output {
        self? - other?
    }
}

// Decimal - decimal

impl Sub<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn sub(self, other: Decimal) -> Self::Output {
        self.0
            .checked_sub(other.0)
            .map(Self)
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
            .0
            .checked_sub(other.0)
            .map(Decimal)
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
