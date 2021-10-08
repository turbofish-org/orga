use super::{Amount, Coin};
use crate::state::State;
use ed::{Decode, Encode};

pub trait Symbol: Sized + State {
    fn mint<I: Into<Amount<Self>>>(amount: I) -> Coin<Self> {
        Coin::mint(amount)
    }
}
