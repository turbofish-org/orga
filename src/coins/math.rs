use super::Ratio;
use crate::{Error, Result};
use std::convert::{TryFrom, TryInto};
use std::ops::{ControlFlow, FromResidual, Try};
use std::result::Result as StdResult;

pub enum MathResult<T> {
    Ok(T),
    Err(Error),
}

impl<T> From<MathResult<T>> for Result<T> {
    fn from(_residual: MathResult<T>) -> Self {
        Self::Err(Error::Unknown)
    }
}

impl<T, U, E: std::error::Error> FromResidual<StdResult<U, E>> for MathResult<T> {
    fn from_residual(_residual: StdResult<U, E>) -> Self {
        MathResult::Err(Error::Unknown)
    }
}

impl<T> FromResidual<MathResult<!>> for Result<T> {
    fn from_residual(_residual: MathResult<!>) -> Self {
        Result::Err(Error::Unknown)
    }
}

impl<T> FromResidual<MathResult<!>> for MathResult<T> {
    fn from_residual(residual: MathResult<!>) -> Self {
        match residual {
            MathResult::Err(err) => MathResult::Err(err),
            _ => unreachable!(),
        }
    }
}

impl<T> Try for MathResult<T> {
    type Output = T;
    type Residual = MathResult<!>;

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            MathResult::Ok(value) => ControlFlow::Continue(value),
            MathResult::Err(_err) => ControlFlow::Break(MathResult::Err(Error::Unknown)),
        }
    }

    fn from_output(output: Self::Output) -> Self {
        MathResult::Ok(output)
    }
}
