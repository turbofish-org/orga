//! Token denoms and metadata.

use super::{Amount, Coin};
use crate::{migrate::Migrate, state::State};

/// A type that uniquely identifies a token, with an associated name and
/// fixed identifier byte.
pub trait Symbol:
    Sized + State + std::fmt::Debug + 'static + Clone + Send + Default + Migrate + Send + Sync
{
    /// Identifier byte.
    const INDEX: u8;
    /// Human-readable symbol name.
    const NAME: &'static str;
    /// Mint a coin with this symbol.
    fn mint<I: Into<Amount>>(amount: I) -> Coin<Self> {
        Coin::mint(amount)
    }
}
