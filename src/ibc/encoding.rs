use crate::encoding::{Decode, Encode, Terminated};
use crate::state::State;
use crate::store::Store;
use prost_types::Any;
use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

#[derive(Clone)]
pub struct Adapter<T> {
    inner: T,
}

impl<T> Encode for Adapter<T>
where
    T: Serialize,
{
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.encode()?.len())
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let bytes = bincode::serialize(&self.inner).map_err(|_e| ed::Error::UnexpectedByte(0))?;
        let len = bytes.len() as u16;
        dest.write_all(&len.encode()?)?;
        dest.write_all(&bytes)?;

        Ok(())
    }
}

impl<'a, T> Decode for Adapter<T>
where
    T: for<'de> Deserialize<'de>,
{
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let len = u16::decode(&mut reader)?;
        let mut bytes = vec![0u8; len as usize];
        reader.read_exact(&mut bytes)?;
        let inner: T =
            bincode::deserialize_from(reader).map_err(|_e| ed::Error::UnexpectedByte(0))?;

        Ok(Self { inner })
    }
}

impl<T> Terminated for Adapter<T> {}

impl<T> From<T> for Adapter<T> {
    fn from(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> Adapter<T> {
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> std::ops::Deref for Adapter<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for Adapter<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T> State for Adapter<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> crate::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> crate::Result<Self::Encoding> {
        Ok(self)
    }
}

// Protobuf adapter

pub struct ProtobufAdapter<T> {
    inner: T,
}

impl<T> Encode for ProtobufAdapter<T>
where
    T: Protobuf<Any>,
    Any: From<T>,
    <T as std::convert::TryFrom<Any>>::Error: std::fmt::Display,
{
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.encode()?.len())
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let mut bytes = vec![];
        self.inner
            .encode(&mut bytes)
            .map_err(|e| ed::Error::UnexpectedByte(0))?;
        let len = bytes.len() as u16;

        dest.write_all(&len.encode()?)?;
        dest.write_all(&bytes)?;

        Ok(())
    }
}

impl<T> Decode for ProtobufAdapter<T>
where
    T: Protobuf<Any>,
    Any: From<T>,
    <T as std::convert::TryFrom<Any>>::Error: std::fmt::Display,
{
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let len = u16::decode(&mut reader)?;
        let mut bytes = vec![0u8; len as usize];
        reader.read_exact(&mut bytes)?;

        let inner: T = T::decode(bytes.as_slice()).map_err(|_e| ed::Error::UnexpectedByte(0))?;

        Ok(Self { inner })
    }
}

impl<T> Terminated for ProtobufAdapter<T> {}

impl<T> From<T> for ProtobufAdapter<T> {
    fn from(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> ProtobufAdapter<T> {
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> std::ops::Deref for ProtobufAdapter<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for ProtobufAdapter<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> State for ProtobufAdapter<T>
where
    T: Protobuf<Any>,
    Any: From<T>,
    <T as std::convert::TryFrom<Any>>::Error: std::fmt::Display,
{
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> crate::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> crate::Result<Self::Encoding> {
        Ok(self)
    }
}
