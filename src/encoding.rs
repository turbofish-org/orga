use crate::describe::Describe;
use crate::migrate::MigrateFrom;
use crate::query::FieldQuery;
use crate::state::State;
use crate::store::Store;
pub use ed::*;
pub use orga_macros::VersionedEncoding;
pub mod decoder;
pub mod encoder;

use derive_more::{Deref, DerefMut, Into};
use std::convert::{TryFrom, TryInto};

#[derive(
    Deref,
    DerefMut,
    Encode,
    Into,
    Default,
    Clone,
    Debug,
    FieldQuery,
    MigrateFrom,
    PartialEq,
    Hash,
    Eq,
    Describe,
)]
pub struct LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: Encode + Decode + Terminated,
{
    len: P,

    #[deref]
    #[deref_mut]
    #[into]
    values: Vec<T>,
}

impl<P, T> LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: Encode + Decode + Terminated,
{
    pub fn new(len: P, values: Vec<T>) -> Self {
        LengthVec { len, values }
    }
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

impl<P, T> Terminated for LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: Encode + Decode + Terminated,
{
}

impl<P, T> State for LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: Encode + Decode + Terminated,
    Self: 'static,
{
    fn attach(&mut self, _store: Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.len.encode_into(out)?;
        self.values.encode_into(out)?;

        Ok(())
    }

    fn load(_store: Store, mut bytes: &mut &[u8]) -> crate::Result<Self> {
        let len = P::decode(&mut bytes)?;
        let len_usize = len
            .clone()
            .try_into()
            .map_err(|_| Error::UnexpectedByte(80))?;

        let mut values = Vec::with_capacity(len_usize);
        for _ in 0..len_usize {
            let value = T::decode(&mut bytes)?;
            values.push(value);
        }

        Ok(LengthVec { len, values })
    }
}

// impl<P, T> Describe for LengthVec<P, T>
// where
//     P: State + Encode + Decode + TryInto<usize> + Terminated + Clone + 'static,
//     T: State + Encode + Decode + Terminated + 'static,
// {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>().build()
//     }
// }

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

pub struct Adapter<T>(pub T);

impl<T> State for Adapter<T>
where
    Self: Encode + Decode + 'static,
{
    fn attach(&mut self, _store: crate::store::Store) -> crate::Result<()> {
        Ok(())
    }
    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.encode_into(out)?;
        Ok(())
    }
    fn load(_store: crate::store::Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        Ok(Self::decode(bytes)?)
    }
}
