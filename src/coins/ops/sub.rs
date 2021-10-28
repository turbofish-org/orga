use super::super::{Amount, MathResult, Ratio};
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
