use super::{Amount, Symbol};
use crate::Result;
pub trait Balance<S: Symbol, U = Amount> {
    fn balance(&self) -> Result<U>;
}
