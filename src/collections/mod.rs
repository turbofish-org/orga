use crate::encoding::{Decode, Encode};

mod deque;
mod map;
mod set;

pub use deque::Deque;
pub use map::Map;
pub use set::Set;

/// A trait for types which can be converted to or from a key/value pair.
pub trait Entry {
    type Key: Encode + Decode;
    type Value: Encode + Decode;

    fn into_entry(self) -> (Self::Key, Self::Value);
    fn from_entry(entry: (Self::Key, Self::Value)) -> Self;
}
