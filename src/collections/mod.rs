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

pub trait Next<T> {
    fn next(&self) -> Option<T>;
}

macro_rules! impl_next {
    ($T:ty) => {
        impl Next<$T> for $T {
            fn next(&self) -> Option<Self> {
                self.checked_add(1)
            }
        }
    };
}

impl_next!(u8);
impl_next!(u16);
impl_next!(u32);
impl_next!(u64);
impl_next!(u128);
impl_next!(i8);
impl_next!(i16);
impl_next!(i32);
impl_next!(i64);
impl_next!(i128);

impl<T, const N: usize> Next<[T; N]> for [T; N]
where
    T: Default + Next<T> + Copy,
{
    fn next(&self) -> Option<[T; N]> {
        let mut return_key: [T; N] = *self;
        for (i, value) in self.iter().enumerate().rev() {
            match <T>::next(value) {
                Some(new_value) => {
                    return_key[i] = new_value;
                    return Some(return_key);
                }
                None => {
                    return_key[i] = T::default();
                }
            }
        }
        None
    }
}

#[allow(unused_imports)]
mod test {
    use super::*;

    #[test]
    fn u8_next() {
        let key: [u8; 3] = [2, 3, 0];
        assert_eq!(key.next().unwrap(), [2, 3, 1]);
    }

    #[test]
    fn u8_next_last_max() {
        let key: [u8; 3] = [2, 3, 255];
        assert_eq!(key.next().unwrap(), [2, 4, 0]);
    }

    #[test]
    fn u8_next_all_max() {
        let key: [u8; 3] = [255, 255, 255];
        assert_eq!(key.next(), None);
    }
}
