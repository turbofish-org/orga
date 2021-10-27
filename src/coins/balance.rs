use super::Amount;

pub trait Balance {
    fn balance(&self) -> Amount;
}
