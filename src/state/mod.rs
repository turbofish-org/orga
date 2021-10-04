use crate::store::*;
use crate::Result;
use ed::{Decode, Encode};
pub use orga_macros::State;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
///
/// These types can be complex types like collections (e.g. maps), or simple
/// data types (e.g. account structs).
pub trait State<S = DefaultBackingStore>: Sized {
    /// A type which provides the binary encoding of data stored in the type's
    /// root key/value entry. When being written to a store, `State` values will
    /// be converted to this type and then encoded into bytes. When being
    /// loaded, bytes will be decoded as this type and then passed to `create()`
    /// along with access to the store to construct the `State` value.
    ///
    /// For complex types which store all of their data in child key/value
    /// entries, this will often be the unit tuple (`()`) in order to provide a
    /// no-op encoding.
    ///
    /// For simple data types, this will often be `Self` since a separate type
    /// is not needed.
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    /// Creates an instance of the type from a dedicated substore (`store`) and
    /// associated data (`data`).
    ///
    /// Implementations which only represent simple data and do not need access
    /// to the store can just ignore the `store` argument.
    ///
    /// This method will be called by some external container and will rarely be
    /// explicitly called to construct an instance of the type.
    fn create(store: Store<S>, data: Self::Encoding) -> Result<Self>
    where
        S: Read;

    /// Called when the data is to be written to the backing store, and converts
    /// the instance into `Self::Encoding` in order to specify how it should be
    /// represented in binary bytes.
    ///
    /// Note that the type does not write its own binary representation, it is
    /// assumed some external container will store the bytes in a relevant part
    /// of the store. The type does however need to write any child key/value
    /// entries (often by calling child `State` types' `flush()` implementations
    /// then storing their resulting binary representations) to the store if
    /// necessary.
    fn flush(self) -> Result<Self::Encoding>
    where
        S: Write;
}

macro_rules! state_impl {
    ($type:ty) => {
        impl<S> State<S> for $type {
            type Encoding = Self;

            #[inline]
            fn create(_: Store<S>, value: Self) -> Result<Self> {
                Ok(value)
            }

            #[inline]
            fn flush(self) -> Result<Self::Encoding> {
                Ok(self)
            }
        }
    };
}

state_impl!(u8);
state_impl!(u16);
state_impl!(u32);
state_impl!(u64);
state_impl!(u128);
state_impl!(bool);
state_impl!(());

impl<T: ed::Encode + ed::Decode + ed::Terminated, S, const N: usize> State<S> for [T; N] {
    type Encoding = Self;

    fn create(_: Store<S>, value: Self::Encoding) -> Result<Self> {
        Ok(value)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

#[derive(Encode, Decode)]
pub struct EncodedOption<T: State<S>, S> {
    inner: Option<T::Encoding>,
}

impl<T: State<S>, S> From<Option<T>> for EncodedOption<T, S> {
    fn from(option: Option<T>) -> Self {
        match option {
            Some(inner) => EncodedOption {
                inner: Some(inner.into()),
            },
            None => EncodedOption { inner: None },
        }
    }
}

impl<T: State<S>, S> State<S> for Option<T> {
    type Encoding = EncodedOption<T, S>;

    fn create(store: Store<S>, value: Self::Encoding) -> Result<Self>
    where
        S: Read,
    {
        match value.inner {
            Some(inner) => {
                let upcast = T::create(store, inner)?;
                Ok(Some(upcast))
            }
            None => Ok(None),
        }
    }

    fn flush(self) -> Result<Self::Encoding> {
        match self {
            Some(inner) => Ok(EncodedOption {
                inner: Some(inner.into()),
            }),
            None => Ok(EncodedOption { inner: None }),
        }
    }
}

macro_rules! state_tuple_impl {
    ($($type:ident),*; $($length:tt),*; $new_type_name:tt) => {

        #[derive(Encode, Decode)]
        pub struct $new_type_name <$($type, )* S>
        where
            $($type: State<S>,)* {
                inner: ($($type::Encoding,)*),

        }

        impl<$($type,)* S> From<($($type,)*)> for $new_type_name<$($type, )* S>
        where
            $($type: State<S>,)* {
                fn from(value: ($($type,)*)) -> Self {
                    $new_type_name {
                        inner: ($(value.$length.into(),)*),
                    }
                }
            }

        impl<$($type: State<S>,)* S> State<S> for ($($type,)*)
        where
            $($type::Encoding: ed::Terminated,)*{
            type Encoding = $new_type_name<$($type,)* S>;

            fn create(store: Store<S>, data: Self::Encoding) -> Result<Self>
            where
                S: Read,
            {
                Ok(($($type::create(store.clone(), data.inner.$length)?,)*))
            }

            fn flush(self) -> Result<Self::Encoding> {
                Ok($new_type_name {
                    inner: ($(self.$length.into(),)*),
                })
            }

        }
    }
}

state_tuple_impl!(A; 0; Encoded1Tuple);
state_tuple_impl!(A, B; 0, 1; Encoded2Tuple);
state_tuple_impl!(A, B, C; 0, 1, 2; Encoded3Tuple);
state_tuple_impl!(A, B, C, D; 0, 1, 2, 3; Encoded4Tuple);
state_tuple_impl!(A, B, C, D, E; 0, 1, 2, 3, 4; Encoded5Tuple);
state_tuple_impl!(A, B, C, D, E, F; 0, 1, 2, 3, 4, 5; Encoded6Tuple);
state_tuple_impl!(A, B, C, D, E, F, G; 0, 1, 2, 3, 4, 5, 6; Encoded7Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H; 0, 1, 2, 3, 4, 5, 6, 7; Encoded8Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I; 0, 1, 2, 3, 4, 5, 6, 7, 8; Encoded9Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9; Encoded10Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10; Encoded11Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K, L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11; Encoded12Tuple);
