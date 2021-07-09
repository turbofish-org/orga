use crate::encoding::{Decode, Encode};

pub mod deque;
pub mod map;

pub use deque::Deque;
pub use map::Map;

/// A trait for types which can be converted to or from a key/value pair.
///
/// This is useful when it is conceptually easier to think about values as
/// singular structs while storing them in a map as a key/value pair.
pub trait Entry {
    /// Represents the key type for the key/value pair.
    type Key: Encode + Decode;

    /// Represents the value type for the key/value pair.
    type Value: Encode + Decode;

    /// Converts the entry type into its corresponding key and value types.
    fn into_entry(self) -> (Self::Key, Self::Value);

    /// Converts from the key and value types into the entry type.
    fn from_entry(entry: (Self::Key, Self::Value)) -> Self;
}
