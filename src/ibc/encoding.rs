use crate::encoding::{Decode, Encode, Terminated};
use crate::state::State;
use crate::store::Store;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
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
        let mut bytes: Vec<u8> = vec![];
        self.encode_into(&mut bytes)?;

        Ok(bytes.len())
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
        let inner: T = bincode::deserialize_from(bytes.as_slice())
            .map_err(|_e| ed::Error::UnexpectedByte(0))?;

        Ok(Self { inner })
    }
}

impl<T> Terminated for Adapter<T> {}

impl<T> From<T> for Adapter<T>
where
    T: IbcProto,
{
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

pub struct ProtobufAdapter<T, P> {
    inner: T,
    _pb: PhantomData<P>,
}

impl<T, P> Encode for ProtobufAdapter<T, P>
where
    T: Protobuf<P>,
    P: From<T> + Default + Message,
    <T as std::convert::TryFrom<P>>::Error: std::fmt::Display,
{
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.inner.encoded_len())
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

impl<T, P> Decode for ProtobufAdapter<T, P>
where
    T: Protobuf<P>,
    P: From<T> + Default + Message,
    <T as std::convert::TryFrom<P>>::Error: std::fmt::Display,
{
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let len = u16::decode(&mut reader)?;
        let mut bytes = vec![0u8; len as usize];
        reader.read_exact(&mut bytes)?;

        let inner: T = T::decode(bytes.as_slice()).map_err(|_e| ed::Error::UnexpectedByte(0))?;

        Ok(Self {
            inner,
            _pb: PhantomData,
        })
    }
}

impl<T, P> Terminated for ProtobufAdapter<T, P> {}

trait IbcProto {}

impl IbcProto for ibc::core::ics02_client::client_consensus::AnyConsensusState {}
impl IbcProto for ibc::core::ics24_host::identifier::ClientId {}
impl IbcProto for ibc::core::ics02_client::client_type::ClientType {}
impl IbcProto for ibc::core::ics02_client::client_state::AnyClientState {}
impl IbcProto for ibc::clients::ics07_tendermint::consensus_state::ConsensusState {}
impl IbcProto for ibc::timestamp::Timestamp {}
impl IbcProto for ibc::Height {}
impl<A, B> IbcProto for (A, B)
where
    A: IbcProto,
    B: IbcProto,
{
}

impl<T, P> From<T> for ProtobufAdapter<T, P>
where
    T: IbcProto,
{
    fn from(inner: T) -> Self {
        Self {
            inner,
            _pb: PhantomData,
        }
    }
}

impl<T, P> ProtobufAdapter<T, P> {
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T, P> std::ops::Deref for ProtobufAdapter<T, P> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, P> std::ops::DerefMut for ProtobufAdapter<T, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T, P> State for ProtobufAdapter<T, P>
where
    T: Protobuf<P>,
    P: From<T> + Default + Message,
    <T as std::convert::TryFrom<P>>::Error: std::fmt::Display,
{
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> crate::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> crate::Result<Self::Encoding> {
        Ok(self)
    }
}
