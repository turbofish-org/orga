use super::super::{Amount, Decimal, MathResult, Ratio};
use crate::Error;
use num_traits::CheckedAdd;
use std::ops::Add;

// Amount + amount
impl Add<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn add(self, other: Amount) -> Self::Output {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Add<Amount> for MathResult<Amount> {
    type Output = MathResult<Amount>;

    fn add(self, other: Amount) -> Self::Output {
        self? + other
    }
}

impl Add<MathResult<Amount>> for Amount {
    type Output = MathResult<Amount>;

    fn add(self, other: MathResult<Amount>) -> Self::Output {
        self + other?
    }
}

impl Add<MathResult<Amount>> for MathResult<Amount> {
    type Output = MathResult<Amount>;

    fn add(self, other: MathResult<Amount>) -> Self::Output {
        self? + other?
    }
}

// Amount + ratio

impl Add<Ratio> for Amount {
    type Output = MathResult<Ratio>;

    fn add(self, other: Ratio) -> Self::Output {
        let self_ratio: Ratio = self.into();

        self_ratio
            .0
            .checked_add(&other.0)
            .map(Ratio)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Add<Ratio> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn add(self, other: Ratio) -> Self::Output {
        self? + other
    }
}

impl Add<MathResult<Ratio>> for Amount {
    type Output = MathResult<Ratio>;

    fn add(self, other: MathResult<Ratio>) -> Self::Output {
        self + other?
    }
}

impl Add<MathResult<Ratio>> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn add(self, other: MathResult<Ratio>) -> Self::Output {
        self? + other?
    }
}

// Ratio + ratio

impl Add<Ratio> for Ratio {
    type Output = MathResult<Ratio>;

    fn add(self, other: Ratio) -> Self::Output {
        self.0
            .checked_add(&other.0)
            .map(Ratio)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Add<Ratio> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn add(self, other: Ratio) -> Self::Output {
        self? + other
    }
}

impl Add<MathResult<Ratio>> for Ratio {
    type Output = MathResult<Ratio>;

    fn add(self, other: MathResult<Ratio>) -> Self::Output {
        self + other?
    }
}

impl Add<MathResult<Ratio>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn add(self, other: MathResult<Ratio>) -> Self::Output {
        self? + other?
    }
}

// Ratio + amount

impl Add<Amount> for Ratio {
    type Output = MathResult<Ratio>;

    fn add(self, other: Amount) -> Self::Output {
        other + self
    }
}

impl Add<Amount> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn add(self, other: Amount) -> Self::Output {
        other + self?
    }
}

impl Add<MathResult<Amount>> for Ratio {
    type Output = MathResult<Ratio>;

    fn add(self, other: MathResult<Amount>) -> Self::Output {
        other? + self
    }
}

impl Add<MathResult<Amount>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn add(self, other: MathResult<Amount>) -> Self::Output {
        other? + self?
    }
}

// Decimal + decimal
impl Add<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn add(self, other: Decimal) -> Self::Output {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Add<Decimal> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn add(self, other: Decimal) -> Self::Output {
        self? + other
    }
}

impl Add<MathResult<Decimal>> for Decimal {
    type Output = MathResult<Decimal>;

    fn add(self, other: MathResult<Decimal>) -> Self::Output {
        self + other?
    }
}

impl Add<MathResult<Decimal>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn add(self, other: MathResult<Decimal>) -> Self::Output {
        self? + other?
    }
}

// Amount + decimal

impl Add<Decimal> for Amount {
    type Output = MathResult<Decimal>;

    fn add(self, other: Decimal) -> Self::Output {
        let self_decimal: Decimal = self.into();

        self_decimal
            .0
            .checked_add(other.0)
            .map(Decimal)
            .ok_or(Error::Overflow)
            .into()
    }
}

impl Add<Decimal> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn add(self, other: Decimal) -> Self::Output {
        self? + other
    }
}

impl Add<MathResult<Decimal>> for Amount {
    type Output = MathResult<Decimal>;

    fn add(self, other: MathResult<Decimal>) -> Self::Output {
        self + other?
    }
}

impl Add<MathResult<Decimal>> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn add(self, other: MathResult<Decimal>) -> Self::Output {
        self? + other?
    }
}
