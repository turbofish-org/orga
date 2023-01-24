use super::super::{Amount, Decimal, MathResult};
use crate::Error;
use std::ops::Add;

// Amount + amount
impl Add<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn add(self, other: Amount) -> Self::Output {
        self.value
            .checked_add(other.value)
            .map(|value| value.into())
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

// Decimal + decimal
impl Add<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn add(self, other: Decimal) -> Self::Output {
        self.value
            .checked_add(other.value)
            .map(|value| value.into())
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
            .value
            .checked_add(other.value)
            .map(|value| value.into())
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

// Decimal + amount

impl Add<Amount> for Decimal {
    type Output = MathResult<Decimal>;

    fn add(self, other: Amount) -> Self::Output {
        other + self
    }
}

impl Add<Amount> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn add(self, other: Amount) -> Self::Output {
        other + self?
    }
}

impl Add<MathResult<Amount>> for Decimal {
    type Output = MathResult<Decimal>;

    fn add(self, other: MathResult<Amount>) -> Self::Output {
        other? + self
    }
}

impl Add<MathResult<Amount>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn add(self, other: MathResult<Amount>) -> Self::Output {
        other? + self?
    }
}
