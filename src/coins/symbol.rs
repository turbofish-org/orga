use super::{Amount, Coin};
use crate::state::State;

pub trait Symbol: Sized + State + std::fmt::Debug {
    fn mint<I: Into<Amount>>(amount: I) -> Coin<Self> {
        Coin::mint(amount)
    }
}
