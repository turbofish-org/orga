use super::super::{Amount, MathResult, Ratio};
use crate::Error;
use num_traits::CheckedAdd;
use std::ops::Add;

// Amount + amount
impl Add<Amount> for Amount {
    type Output = MathResult<Amount>;

    fn add(self, other: Amount) -> Self::Output {
        MathResult::Ok(self.0.checked_add(other.0).ok_or(Error::Overflow)?.into())
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
