use crate::client::Client;
use crate::query::Query;
use crate::state::State;
pub use ed::*;

use derive_more::{Deref, DerefMut, Into};
use std::convert::{TryFrom, TryInto};

#[derive(Deref, DerefMut, Encode, Into, Default, Clone, Debug, State, Query, Client)]
pub struct LengthVec<P, T>
where
    P: State + Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: State + Encode + Decode + Terminated,
{
    len: P,

    #[deref]
    #[deref_mut]
    #[into]
    values: Vec<T>,
}

impl<P, T> LengthVec<P, T>
where
    P: State + Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: State + Encode + Decode + Terminated,
{
    pub fn new(len: P, values: Vec<T>) -> Self {
        LengthVec { len, values }
    }
}

impl<P, T> Decode for LengthVec<P, T>
where
    P: State + Encode + Decode + Terminated + TryInto<usize> + Clone,
    T: State + Encode + Decode + Terminated,
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

impl<P, T> Terminated for LengthVec<P, T>
where
    P: State + Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: State + Encode + Decode + Terminated,
{
}

impl<P, T> TryFrom<Vec<T>> for LengthVec<P, T>
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
    T: State + Encode + Decode + Terminated,
{
    type Error = crate::Error;

    fn try_from(values: Vec<T>) -> crate::Result<Self> {
        let len = values
            .len()
            .try_into()
            .map_err(|_| crate::Error::Overflow)?;
        Ok(Self { len, values })
    }
}
