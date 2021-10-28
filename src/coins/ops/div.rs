use super::super::{Amount, MathResult, Ratio};
use crate::Error;
use num_traits::CheckedDiv;
use std::ops::Div;

// Amount / amount
impl Div<Amount> for Amount {
    type Output = MathResult<Ratio>;

    fn div(self, other: Amount) -> Self::Output {
        Ratio::new(self.0, other.0).into()
    }
}

impl Div<Amount> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

    fn div(self, other: Amount) -> Self::Output {
        self? / other
    }
}

impl Div<MathResult<Amount>> for Amount {
    type Output = MathResult<Ratio>;

    fn div(self, other: MathResult<Amount>) -> Self::Output {
        self / other?
    }
}

impl Div<MathResult<Amount>> for MathResult<Amount> {
    type Output = MathResult<Ratio>;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn amount_div() -> Result<()> {
        let fifty: Amount = 50.into();
        let hundred: Amount = 100.into();
        let four: Amount = 4.into();
        let half = Ratio::new(1, 2)?;
        let quotient = (fifty / hundred / four / half)?;

        let target = Ratio::new(1, 4)?;
        assert_eq!(quotient.0, target.0);

        Ok(())
    }
}
