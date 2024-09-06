use crate::encoding::{Decode, Encode};
use crate::state::State;

pub use crate::macros::{Entry, Next};

pub mod deque;
pub mod entry_map;
pub mod map;

pub use deque::Deque;
pub use entry_map::EntryMap;
pub use map::Map;

pub use map::{ChildMut, Ref};

/// A trait for types which can be converted to or from a key/value pair.
///
/// This is useful when it is conceptually easier to think about values as
/// singular structs while storing them in a map as a key/value pair.
pub trait Entry: 'static {
    /// Represents the key type for the key/value pair.
    type Key: Encode + Decode + Send + Sync;

    /// Represents the value type for the key/value pair.
    type Value: State + Send + Sync;

    /// Converts the entry type into its corresponding key and value types.
    fn into_entry(self) -> (Self::Key, Self::Value);

    /// Converts from the key and value types into the entry type.
    fn from_entry(entry: (Self::Key, Self::Value)) -> Self;
}

/// A trait for types which have a logical next sequential value.
pub trait Next: Sized {
    /// Returns the next sequential value if it exists, or `None` if the value
    /// is at the end of its domain, e.g. `u32::MAX -> None`, or
    /// `0 -> Some(1)`).
    fn next(&self) -> Option<Self>;
}

impl Next for bool {
    fn next(&self) -> Option<Self> {
        match self {
            false => Some(true),
            true => None,
        }
    }
}

macro_rules! impl_next {
    ($T:ty) => {
        impl Next for $T {
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

macro_rules! tuple_next {
    ($($type:ident),*; $($length:tt),*) => {
        impl<$($type: Next + Default + Clone,)*> Next for ($($type,)*) {
            fn next(&self) -> Option<Self> {
                    let mut return_tuple: ($($type,)*) = self.clone();

                    $(match self.$length.next() {
                        Some(value) => {
                            return_tuple.$length = value;
                            return Some(return_tuple);
                        }
                        None => {
                            return_tuple.$length = Default::default();
                        }
                    })*
                None
            }
        }
    }
}

tuple_next!(A; 0);
tuple_next!(A, B; 1, 0);
tuple_next!(A, B, C; 2, 1, 0);
tuple_next!(A, B, C, D; 3, 2, 1, 0);
tuple_next!(A, B, C, D, E; 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F; 5, 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F, G; 6, 5, 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F, G, H; 7, 6, 5, 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F, G, H, I; 8, 7, 6, 5, 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F, G, H, I, J; 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F, G, H, I, J, K; 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
tuple_next!(A, B, C, D, E, F, G, H, I, J, K, L; 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);

impl<T, const N: usize> Next for [T; N]
where
    T: Default + Next + Clone,
{
    fn next(&self) -> Option<[T; N]> {
        let mut return_key: [T; N] = self.clone();
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

    #[test]
    fn two_tuple_next() {
        let key: (u8, u32) = (0, 0);
        assert_eq!(key.next().unwrap(), (0, 1));
    }

    #[test]
    fn two_tuple_next_last_max() {
        let key: (u8, u8) = (0, 255);
        assert_eq!(key.next().unwrap(), (1, 0));
    }

    #[test]
    fn two_tuple_next_all_max() {
        let key: (u8, u8) = (255, 255);
        assert!(key.next().is_none());
    }

    #[test]
    fn single_tuple_next() {
        let key: (u8,) = (0,);
        assert_eq!(key.next().unwrap(), (1,));
    }

    #[test]
    fn single_tuple_max() {
        let key: (u8,) = (255,);
        assert!(key.next().is_none());
    }

    #[test]
    fn max_tuple_next() {
        let key: (u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8) =
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(key.next().unwrap(), (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1));
    }
}
