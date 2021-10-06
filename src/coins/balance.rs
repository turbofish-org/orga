use super::{Amount, Symbol};

pub trait Balance<S: Symbol> {
    fn balance(&self) -> Amount<S>;
}
