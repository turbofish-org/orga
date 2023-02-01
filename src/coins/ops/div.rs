use super::super::{Amount, Decimal, MathResult};
use crate::Error;
use std::ops::Div;

// Amount / amount
impl Div<Amount> for Amount {
    type Output = MathResult<Decimal>;

    fn div(self, other: Amount) -> Self::Output {
        let self_dec: Decimal = self.into();
        let other_dec: Decimal = other.into();

        self_dec / other_dec
    }
}

impl Div<Amount> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn div(self, other: Amount) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Amount>> for Amount {
    type Output = MathResult<Decimal>;

    fn div(self, other: MathResult<Amount>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Amount>> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn div(self, other: MathResult<Amount>) -> Self::Output {
        self? / other?
    }
}

// Decimal / decimal
impl Div<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn div(self, other: Decimal) -> Self::Output {
        self.value
            .checked_div(other.value)
            .map(|value| value.into())
            .ok_or(Error::DivideByZero)
            .into()
    }
}

impl Div<Decimal> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn div(self, other: Decimal) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Decimal>> for Decimal {
    type Output = MathResult<Decimal>;

    fn div(self, other: MathResult<Decimal>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Decimal>> for MathResult<Decimal> {
    type Output = MathResult<Decimal>;

    fn div(self, other: MathResult<Decimal>) -> Self::Output {
        self? / other?
    }
}

// Amount / decimal
impl Div<Decimal> for Amount {
    type Output = MathResult<Decimal>;

    fn div(self, other: Decimal) -> Self::Output {
        let self_decimal: Decimal = self.into();

        self_decimal
            .value
            .checked_div(other.value)
            .map(|value| value.into())
            .ok_or(Error::DivideByZero)
            .into()
    }
}

impl Div<Decimal> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn div(self, other: Decimal) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Decimal>> for Amount {
    type Output = MathResult<Decimal>;

    fn div(self, other: MathResult<Decimal>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Decimal>> for MathResult<Amount> {
    type Output = MathResult<Decimal>;

    fn div(self, other: MathResult<Decimal>) -> Self::Output {
        self? / other?
    }
}
