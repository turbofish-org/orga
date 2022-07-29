use crate::state::State;
pub use ed::*;

use derive_more::{Deref, DerefMut, Into};
use std::convert::{TryFrom, TryInto};

#[derive(Deref, DerefMut, Encode, Into, Default, Clone)]
pub struct LengthVec<P, T>
where
    P: Encode + Terminated,
    T: Encode + Terminated,
{
    len: P,

    #[deref]
    #[deref_mut]
    #[into]
    values: Vec<T>,
}

impl<P, T> LengthVec<P, T>
where
    P: Encode + Terminated,
    T: Encode + Terminated,
{
    pub fn new(len: P, values: Vec<T>) -> Self {
        LengthVec { len, values }
    }
}

impl<P, T> State for LengthVec<P, T>
where
    P: Encode + Decode + Terminated + TryInto<usize> + Clone,
    T: Encode + Decode + Terminated,
{
    type Encoding = Self;

    fn create(_: orga::store::Store, data: Self::Encoding) -> crate::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> crate::Result<Self::Encoding> {
        Ok(self)
    }
}

impl<P, T> From<Vec<T>> for LengthVec<P, T>
where
    P: Encode + Terminated + TryFrom<usize>,
    T: Encode + Terminated,
    <P as TryFrom<usize>>::Error: std::fmt::Debug,
{
    fn from(values: Vec<T>) -> Self {
        LengthVec::new(P::try_from(values.len()).unwrap(), values)
    }
}

impl<P, T> Terminated for LengthVec<P, T>
where
    P: Encode + Terminated,
    T: Encode + Terminated,
{
}

impl<P, T> Decode for LengthVec<P, T>
where
    P: Encode + Decode + Terminated + TryInto<usize> + Clone,
    T: Encode + Decode + Terminated,
{
    fn decode<R: std::io::Read>(mut input: R) -> Result<Self> {
        let len = P::decode(&mut input)?;
        let len_usize = len
            .clone()
            .try_into()
            .map_err(|_| Error::UnexpectedByte(80))?;

        let mut values = Vec::with_capacity(len_usize);
        for _ in 0..len_usize {
            let value = T::decode(&mut input)?;
            values.push(value);
        }

        Ok(LengthVec { len, values })
    }
}
