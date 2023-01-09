use crate::state::state2::State;

pub trait Entry {
    /// Represents the key type for the key/value pair.
    type Key: State;

    /// Represents the value type for the key/value pair.
    type Value: State;

    /// Converts the entry type into its corresponding key and value types.
    fn into_entry(self) -> (Self::Key, Self::Value);

    /// Converts from the key and value types into the entry type.
    fn from_entry(entry: (Self::Key, Self::Value)) -> Self;
}
