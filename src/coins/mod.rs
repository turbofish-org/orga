pub mod amount;
use std::{fmt::Display, str::FromStr};

pub use amount::*;

pub mod symbol;
pub use symbol::*;

pub mod coin;
pub use coin::*;

pub mod give;
pub use give::*;

pub mod take;
pub use take::*;

pub mod pool;
pub use pool::*;

pub mod adjust;
pub use adjust::*;

pub mod balance;
pub use balance::*;

use bech32::{self, encode_to_fmt, FromBase32, ToBase32, Variant};

use crate::collections::Next;
use crate::encoding::{Decode, Encode};
use crate::query::Query;

#[derive(Encode, Decode, Next, Query, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Copy)]
pub struct Address {
    bytes: [u8; 32],
}

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        encode_to_fmt(f, "nomic", self.bytes.to_base32(), Variant::Bech32m).unwrap()
    }
}

impl FromStr for Address {
    type Err = bech32::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (hrp, data, variant) = bech32::decode(s)?;
        if hrp != "nomic" {
            return Err(bech32::Error::MissingSeparator);
        }
        if variant != Variant::Bech32m {
            return Err(bech32::Error::InvalidData(0));
        }
        let data: Vec<u8> = FromBase32::from_base32(&data)?;

        if data.len() != 32 {
            return Err(bech32::Error::InvalidData(0));
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&data);

        Ok(Address { bytes })
    }
}

impl From<[u8; 32]> for Address {
    fn from(bytes: [u8; 32]) -> Self {
        Address { bytes }
    }
}

impl From<Address> for [u8; 32] {
    fn from(addr: Address) -> Self {
        addr.bytes()
    }
}

impl Address {
    pub fn bytes(&self) -> [u8; 32] {
        self.bytes
    }
}
