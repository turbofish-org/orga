use super::Decimal;
use crate::Result;

pub trait Adjust {
    fn adjust(&mut self, multiplier: Decimal) -> Result<()>;
}
