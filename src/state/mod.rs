use crate::encoding::{Decode, Encode, Terminated};
use crate::store::*;
use crate::{Error, Result};
pub use orga_macros::State;
use std::cell::{RefCell, UnsafeCell};
use std::convert::TryInto;
use std::marker::PhantomData;

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
    type Encoding: Encode + Decode + From<Self>;

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
state_impl!(i8);
state_impl!(i16);
state_impl!(i32);
state_impl!(i64);
state_impl!(i128);
state_impl!(bool);
state_impl!(());

#[derive(Encode, Decode)]
pub struct EncodedArray<T: State<S>, S, const N: usize> {
    inner: [T::Encoding; N],
}

impl<T: State<S>, S, const N: usize> From<[T; N]> for EncodedArray<T, S, N> {
    fn from(value: [T; N]) -> Self {
        Self {
            inner: value.map(|val| val.into()),
        }
    }
}

impl<T: State<S>, S, const N: usize> Default for EncodedArray<T, S, N>
where
    T::Encoding: Default,
{
    fn default() -> Self {
        Self {
            inner: [(); N].map(|_| T::Encoding::default()),
        }
    }
}

impl<T: State<S>, S, const N: usize> State<S> for [T; N]
where
    T::Encoding: ed::Terminated,
{
    type Encoding = EncodedArray<T, S, N>;

    fn create(store: Store<S>, value: Self::Encoding) -> Result<Self>
    where
        S: Read,
    {
        let self_vec: Vec<T::Encoding> = match value.inner.try_into() {
            Ok(inner) => inner,
            _ => {
                return Err(Error::State(
                    "Failed to cast self as Vec<T::Encoding>".into(),
                ))
            }
        };
        let mut vec: Vec<Result<T>> = Vec::with_capacity(N);
        self_vec
            .into_iter()
            .for_each(|x| vec.push(T::create(store.clone(), x)));
        let result: Result<Vec<T>> = vec.into_iter().collect();
        //since vec is directly created and populated from passed value, panic! will never be reached
        let result_array: [T; N] = result?.try_into().unwrap_or_else(|v: Vec<T>| {
            panic!("Expected Vec of length {}, but found length {}", N, v.len())
        });
        Ok(result_array)
    }

    fn flush(self) -> Result<Self::Encoding> {
        let self_vec: Vec<T> = match self.try_into() {
            Ok(inner) => inner,
            _ => return Err(Error::State("Failed to cast self as Vec<T>".into())),
        };
        let mut vec: Vec<T::Encoding> = Vec::with_capacity(N);
        self_vec.into_iter().for_each(|x| vec.push(x.into()));

        //since vec is directly created and populated from passed value, panic! will never be reached
        let result_array: [T::Encoding; N] =
            vec.try_into().unwrap_or_else(|v: Vec<T::Encoding>| {
                panic!("Expected Vec of length {}, but found length {}", N, v.len())
            });

        Ok(EncodedArray {
            inner: result_array,
        })
    }
}

impl<T> State for Vec<T>
where
    T: Encode + Decode + Terminated,
{
    type Encoding = Vec<T>;

    fn create(_: Store, data: Self::Encoding) -> Result<Self> {
        Ok(data)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

#[derive(Encode, Decode, Default)]
pub struct EncodingWrapper<T: Encode + Decode>(T);

impl<T> State for RefCell<T>
where
    T: State,
    T::Encoding: From<T> + Encode + Decode,
{
    type Encoding = EncodingWrapper<T::Encoding>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(RefCell::new(T::create(store, data.0)?))
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(EncodingWrapper(self.into_inner().flush()?))
    }
}

impl<T> From<RefCell<T>> for EncodingWrapper<T::Encoding>
where
    T: State,
    T::Encoding: From<T> + Encode + Decode,
{
    fn from(value: RefCell<T>) -> Self {
        Self(value.into_inner().into())
    }
}

impl<T> State for UnsafeCell<T>
where
    T: State,
    T::Encoding: From<T> + Encode + Decode,
{
    type Encoding = EncodingWrapper<T::Encoding>;

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(UnsafeCell::new(T::create(store, data.0)?))
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(EncodingWrapper(self.into_inner().flush()?))
    }
}

impl<T> From<UnsafeCell<T>> for EncodingWrapper<T::Encoding>
where
    T: State,
    T::Encoding: From<T> + Encode + Decode,
{
    fn from(value: UnsafeCell<T>) -> Self {
        Self(value.into_inner().into())
    }
}

impl<T: State<S>, S> State<S> for PhantomData<T> {
    type Encoding = Self;

    fn create(_: Store<S>, data: Self::Encoding) -> Result<Self> {
        Ok(data)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

#[derive(Encode, Decode)]
pub struct Encoded1Tuple<A, S>
where
    A: State<S>,
{
    inner: (A::Encoding,),
}

impl<A, S> From<(A,)> for Encoded1Tuple<A, S>
where
    A: State<S>,
{
    fn from(value: (A,)) -> Self {
        Encoded1Tuple {
            inner: (value.0.into(),),
        }
    }
}

impl<A, S> State<S> for (A,)
where
    A: State<S>,
{
    type Encoding = Encoded1Tuple<A, S>;

    fn create(store: Store<S>, data: Self::Encoding) -> Result<Self>
    where
        S: Read,
    {
        Ok((A::create(store.sub(&[0]), data.inner.0)?,))
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(Encoded1Tuple {
            inner: (self.0.into(),),
        })
    }
}

macro_rules! state_tuple_impl {
    ($($type:ident),*; $last_type: ident; $($indices:tt),*; $length: tt; $new_type_name:tt) => {

        #[derive(Encode, Decode)]
        pub struct $new_type_name <$($type, )* $last_type, S>
        where
            $($type: State<S>,)* $last_type: State<S> {
                inner: ($($type::Encoding,)* $last_type::Encoding),

        }

        impl<$($type,)* $last_type, S> From<($($type,)* $last_type,)> for $new_type_name<$($type, )* $last_type, S>
        where
            $($type: State<S>,)* $last_type: State<S> {
                fn from(value: ($($type,)* $last_type),) -> Self {
                    $new_type_name {
                        inner: ($(value.$indices.into(),)* value.$length.into(),),
                    }
                }
            }

        //last one doesn't necessarily need to be terminated
        impl<$($type,)* $last_type, S> State<S> for ($($type,)* $last_type,)
        where
            $($type: State<S>,)* $last_type: State<S>,
            $($type::Encoding: ed::Terminated,)*{
            type Encoding = $new_type_name<$($type,)* $last_type, S>;

            fn create(store: Store<S>, data: Self::Encoding) -> Result<Self>
            where
                S: Read,
            {
                Ok(($($type::create(store.sub(&[$indices]), data.inner.$indices)?,)* $last_type::create(store.sub(&[$length]), data.inner.$length)?))
            }

            fn flush(self) -> Result<Self::Encoding> {
                Ok($new_type_name {
                    inner: ($(self.$indices.into(),)* self.$length.into(),),
                })
            }
        }
    }
}

state_tuple_impl!(A; B; 0; 1; Encoded2Tuple);
state_tuple_impl!(A, B; C; 0, 1; 2; Encoded3Tuple);
state_tuple_impl!(A, B, C; D; 0, 1, 2; 3; Encoded4Tuple);
state_tuple_impl!(A, B, C, D; E; 0, 1, 2, 3; 4; Encoded5Tuple);
state_tuple_impl!(A, B, C, D, E; F; 0, 1, 2, 3, 4; 5; Encoded6Tuple);
state_tuple_impl!(A, B, C, D, E, F; G; 0, 1, 2, 3, 4, 5; 6; Encoded7Tuple);
state_tuple_impl!(A, B, C, D, E, F, G; H; 0, 1, 2, 3, 4, 5, 6; 7; Encoded8Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H; I; 0, 1, 2, 3, 4, 5, 6, 7; 8; Encoded9Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I; J; 0, 1, 2, 3, 4, 5, 6, 7, 8; 9; Encoded10Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J; K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9; 10; Encoded11Tuple);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K; L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10; 11; Encoded12Tuple);

impl<T: Encode + Decode> State for Option<T> {
    type Encoding = Option<T>;

    fn create(_: Store, data: Self) -> Result<Self> {
        Ok(data)
    }

    fn flush(self) -> Result<Self> {
        Ok(self)
    }
}
