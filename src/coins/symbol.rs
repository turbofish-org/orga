use super::{Amount, Coin};
use crate::{migrate::Migrate, state::State};

pub trait Symbol:
    Sized + State + std::fmt::Debug + 'static + Clone + Send + Default + Migrate + Send + Sync
{
    const INDEX: u8;
    const NAME: &'static str;
    fn mint<I: Into<Amount>>(amount: I) -> Coin<Self> {
        Coin::mint(amount)
    }
}
