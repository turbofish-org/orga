use super::super::{Amount, Decimal, MathResult, Ratio};
use crate::Error;
use num_traits::CheckedDiv;
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

// Amount / ratio

impl Div<Ratio> for Amount {
    type Output = MathResult<Ratio>;

    fn div(self, other: Ratio) -> Self::Output {
        let self_ratio: Ratio = self.into();
        self_ratio / other
    }
}

impl Div<Ratio> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn div(self, other: Ratio) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Ratio>> for Amount {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Ratio>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Ratio>> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Ratio>) -> Self::Output {
        self? / other?
    }
}

// Ratio / ratio

impl Div<Ratio> for Ratio {
    type Output = MathResult<Ratio>;

    fn div(self, other: Ratio) -> Self::Output {
        self.0
            .checked_div(&other.0)
            .map(Ratio)
            .ok_or(Error::DivideByZero)
            .into()
    }
}

impl Div<Ratio> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn div(self, other: Ratio) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Ratio>> for Ratio {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Ratio>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Ratio>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Ratio>) -> Self::Output {
        self? / other?
    }
}

// Ratio / amount

impl Div<Amount> for Ratio {
    type Output = MathResult<Ratio>;

    fn div(self, other: Amount) -> Self::Output {
        let other_ratio: Ratio = other.into();
        self / other_ratio
    }
}

impl Div<Amount> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn div(self, other: Amount) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Amount>> for Ratio {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Amount>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Amount>> for MathResult<Ratio> {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Amount>) -> Self::Output {
        self? / other?
    }
}

// Decimal / decimal

impl Div<Decimal> for Decimal {
    type Output = MathResult<Decimal>;

    fn div(self, other: Decimal) -> Self::Output {
        self.0
            .checked_div(other.0)
            .map(Self)
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
            .0
            .checked_div(other.0)
            .map(Decimal)
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
