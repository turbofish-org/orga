use super::{Amount, Symbol};
pub trait Balance<S: Symbol, U = Amount> {
    fn balance(&self) -> U;
}
