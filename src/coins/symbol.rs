use super::{Amount, Coin};
pub trait Symbol: Sized {
    fn mint<I: Into<Amount<Self>>>(amount: I) -> Coin<Self> {
        Coin::mint(amount)
    }
}
