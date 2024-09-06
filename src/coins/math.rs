//! Safe math ops for coin amounts.
use crate::{Error, Result};
use std::ops::{ControlFlow, FromResidual, Try};
use std::result::Result as StdResult;

/// Result of a mathematical operation.
pub enum MathResult<T> {
    /// The operation was successful and the result is returned.
    Ok(T),
    /// The operation failed and an error is returned.
    Err(Error),
}

impl<T> MathResult<T> {
    /// Converts `MathResult<T>` into `Result<T>`.
    pub fn result(self) -> Result<T> {
        self.into()
    }
}

impl<T> From<MathResult<T>> for Result<T> {
    fn from(math_result: MathResult<T>) -> Self {
        match math_result {
            MathResult::Ok(t) => Ok(t),
            MathResult::Err(err) => Err(err),
        }
    }
}

impl<T> From<Result<T>> for MathResult<T> {
    fn from(result: Result<T>) -> Self {
        match result {
            Ok(t) => MathResult::Ok(t),
            Err(err) => MathResult::Err(err),
        }
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
