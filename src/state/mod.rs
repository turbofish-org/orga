use crate::encoding::{Decode, Encode};
use crate::store::Store;
use crate::{Error, Result};
use ed::Terminated;
pub use orga_macros::State;

mod attach;
pub use attach::Attacher;
mod flush;
pub use flush::Flusher;
mod load;
pub use load::Loader;

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

pub trait State: Sized {
    fn attach(&mut self, store: Store) -> Result<()>;
    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()>;
    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self>;
}

macro_rules! state_impl {
    ($type:ty) => {
        impl State for $type {
            #[inline]
            fn attach(&mut self, _: Store) -> Result<()> {
                Ok(())
            }

            #[inline]
            fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
                Ok(self.encode_into(out)?)
            }

            fn load(_store: Store, bytes: &mut &[u8]) -> Result<Self> {
                Ok(Self::decode(bytes)?)
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

impl<T: State> State for Option<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.as_mut().map_or(Ok(()), |inner| inner.attach(store))
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        match self {
            Some(inner) => {
                out.write_all(&[1])?;
                inner.flush(out)
            }
            None => {
                out.write_all(&[0])?;
                Ok(())
            }
        }
    }

    fn load(store: Store, mut bytes: &mut &[u8]) -> Result<Self> {
        let variant_byte = u8::decode(&mut bytes)?;
        if variant_byte == 0 {
            Ok(None)
        } else {
            Ok(Some(T::load(store, bytes)?))
        }
    }
}

impl<T: State + Terminated, const N: usize> State for [T; N] {
    fn attach(&mut self, store: Store) -> Result<()> {
        for (i, value) in self.iter_mut().enumerate() {
            let prefix = varint(i, N);
            let substore = store.sub(prefix.as_slice());
            value.attach(substore)?;
        }
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        for value in self.into_iter() {
            value.flush(out)?;
        }
        Ok(())
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let items: Vec<T> = (0..N)
            .map(|i| {
                let prefix = varint(i, N);
                let substore = store.sub(prefix.as_slice());
                let value = T::load(substore, bytes)?;
                Ok(value)
            })
            .collect::<Result<_>>()?;

        items
            .try_into()
            .map_err(|_| Error::State(format!("Cannot convert Vec to array of length {}", N)))
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

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        for value in self.into_iter() {
            value.flush(out)?;
        }
        Ok(())
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let mut value = vec![];
        while !bytes.is_empty() {
            let prefix = (value.len() as u64).to_be_bytes();
            let substore = store.sub(prefix.as_slice());
            let item = T::load(substore, bytes)?;
            value.push(item);
        }

        Ok(value)
    }
}

impl<T: State> State for RefCell<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.get_mut().attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        self.into_inner().flush(out)
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        Ok(RefCell::new(T::load(store, bytes)?))
    }
}

impl<T> State for PhantomData<T> {
    fn attach(&mut self, _store: Store) -> Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, _out: &mut W) -> Result<()> {
        Ok(())
    }

    fn load(_store: Store, _bytes: &mut &[u8]) -> Result<Self> {
        Ok(PhantomData::default())
    }
}

macro_rules! state_tuple_impl {
    ($($type:ident),*; $last_type: ident; $($indices:tt),*) => {
        impl<$($type,)* $last_type> State for ($($type,)* $last_type,)
        where
            $($type: State,)*
            $last_type: State,
            // last type doesn't need to be terminated
            $($type: ed::Terminated,)*
        {
            fn attach(&mut self, store: Store) -> Result<()>
            {
                $(self.$indices.attach(store.sub(&[$indices as u8]))?;)*
                Ok(())
            }

            fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()>
            {
                $(self.$indices.flush(out)?;)*
                Ok(())
            }

            fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
                Ok((
                    $(State::load(store.sub(&[$indices as u8]), bytes)?,)*
                ))
            }
        }
    }
}

impl<T: State> State for Rc<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        Rc::<T>::get_mut(self)
            .ok_or_else(|| Error::State("Cannot attach Rc".to_string()))?
            .attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        let value =
            Rc::try_unwrap(self).map_err(|_| Error::State("Cannot flush Rc".to_string()))?;
        value.flush(out)
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let value = T::load(store, bytes)?;

        Ok(Rc::new(value))
    }
}

state_tuple_impl!(; A; 0);
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
