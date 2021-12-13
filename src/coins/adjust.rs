use super::Ratio;
use crate::Result;

pub trait Adjust {
    fn adjust(&mut self, multiplier: Ratio) -> Result<()>;
}
