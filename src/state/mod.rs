use crate::encoding::{Decode, Encode, Terminated};
use crate::store::*;
use crate::{Error, Result};
pub use orga_macros::State;
use serde::{Deserialize, Serialize};
use std::cell::{RefCell, UnsafeCell};
use std::convert::TryInto;
use std::marker::PhantomData;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
///
/// These types can be complex types like collections (e.g. maps), or simple
/// data types (e.g. account structs).
pub trait State<S = DefaultBackingStore>: Encode + Decode + Sized {
    fn attach(&mut self, store: Store<S>) -> Result<()>
    where
        S: Read;

    /// Called when the data is to be written to the backing store.
    ///
    /// Note that the type does not write its own binary representation, it is
    /// assumed some external container will store the bytes in a relevant part
    /// of the store. The type does however need to write any child key/value
    /// entries (often by calling child `State` types' `flush()` implementations
    /// then storing their resulting binary representations) to the store if
    /// necessary.
    fn flush(&mut self) -> Result<()>
    where
        S: Write;
}

macro_rules! state_impl {
    ($type:ty) => {
        impl<S> State<S> for $type {
            #[inline]
            fn attach(&mut self, _: Store<S>) -> Result<()> {
                Ok(())
            }

            #[inline]
            fn flush(&mut self) -> Result<()> {
                Ok(())
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

fn varint(n: usize, max: usize) -> Vec<u8> {
    if max < u8::MAX as usize {
        vec![n as u8]
    } else if max < u16::MAX as usize {
        (n as u16).to_be_bytes().to_vec()
    } else if max < u32::MAX as usize {
        (n as u32).to_be_bytes().to_vec()
    } else {
        (n as u64).to_be_bytes().to_vec()
    }
}

impl<T: State<S> + Terminated, S, const N: usize> State<S> for [T; N] {
    fn attach(&mut self, store: Store<S>) -> Result<()>
    where
        S: Read,
    {
        for (i, value) in self.iter_mut().enumerate() {
            let prefix = varint(i, N);
            let substore = store.sub(prefix.as_slice());
            value.attach(substore)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()>
    where
        S: Write,
    {
        for value in self.iter_mut() {
            value.flush()?;
        }
        Ok(())
    }
}

impl<T: State + Terminated> State for Vec<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        for (i, value) in self.iter_mut().enumerate() {
            let prefix = (i as u64).to_be_bytes();
            let substore = store.sub(prefix.as_slice());
            value.attach(substore)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        for value in self.iter_mut() {
            value.flush()?;
        }
        Ok(())
    }
}

impl<T: State> State for RefCell<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.get_mut().attach(store)
    }

    fn flush(&mut self) -> Result<()> {
        self.get_mut().flush()
    }
}

// impl<T: State> State for UnsafeCell<T> {
//     fn attach(&mut self, store: Store) -> Result<()> {
//         self.get_mut().attach(store)
//     }
//
//     fn flush(&mut self) -> Result<()> {
//         self.get_mut().flush()
//     }
// }

impl<T, S> State<S> for PhantomData<T> {
    fn attach(&mut self, _: Store<S>) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<A: State<S>, S> State<S> for (A,) {
    fn attach(&mut self, store: Store<S>) -> Result<()>
    where
        S: Read,
    {
        self.0.attach(store)
    }

    fn flush(&mut self) -> Result<()>
    where
        S: Write,
    {
        self.0.flush()
    }
}

macro_rules! state_tuple_impl {
    ($($type:ident),*; $last_type: ident; $($indices:tt),*) => {
        impl<$($type,)* $last_type, S> State<S> for ($($type,)* $last_type,)
        where
            $($type: State<S>,)*
            $last_type: State<S>,
            // last type doesn't need to be terminated
            $($type: ed::Terminated,)*
        {
            fn attach(&mut self, store: Store<S>) -> Result<()>
            where
                S: Read,
            {
                $(self.$indices.attach(store.sub(&[$indices as u8]))?;)*
                Ok(())
            }

            fn flush(&mut self) -> Result<()>
            where
                S: Write,
            {
                $(self.$indices.flush()?;)*
                Ok(())
            }
        }
    }
}

state_tuple_impl!(A; B; 0, 1);
state_tuple_impl!(A, B; C; 0, 1, 2);
state_tuple_impl!(A, B, C; D; 0, 1, 2, 3);
state_tuple_impl!(A, B, C, D; E; 0, 1, 2, 3, 4);
state_tuple_impl!(A, B, C, D, E; F; 0, 1, 2, 3, 4, 5);
state_tuple_impl!(A, B, C, D, E, F; G; 0, 1, 2, 3, 4, 5, 6);
state_tuple_impl!(A, B, C, D, E, F, G; H; 0, 1, 2, 3, 4, 5, 6, 7);
state_tuple_impl!(A, B, C, D, E, F, G, H; I; 0, 1, 2, 3, 4, 5, 6, 7, 8);
state_tuple_impl!(A, B, C, D, E, F, G, H, I; J; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J; K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
state_tuple_impl!(A, B, C, D, E, F, G, H, I, J, K; L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);

impl<T: State> State for Option<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.as_mut().map_or(Ok(()), |inner| inner.attach(store))
    }

    fn flush(&mut self) -> Result<()> {
        self.as_mut().map_or(Ok(()), |inner| inner.flush())
    }
}
