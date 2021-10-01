use super::{Amount, Symbol};
use crate::Result;

pub trait Adjust<S: Symbol> {
    fn adjust(&mut self, multiplier: Amount<S>) -> Result<()>;
}
