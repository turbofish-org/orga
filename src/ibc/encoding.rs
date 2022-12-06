use crate::call::Call;
use crate::client::Client;
use crate::collections::Next;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode, Terminated};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics04_channel::packet::Sequence;
use ibc::core::ics04_channel::timeout::TimeoutHeight;
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::signer::Signer;
use ibc::timestamp::Timestamp;
use ibc::Height;
use ibc_proto::ibc::core::channel::v1::PacketState;
use prost::Message;
use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

#[derive(Clone, Call, Client, Query, Debug, Serialize, Deserialize, Default)]
pub struct Adapter<T> {
    pub(crate) inner: T,
}

unsafe impl<T> Send for Adapter<T> {}

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
        // let bytes = bincode::serialize(&self.inner).map_err(|_e|
        // ed::Error::UnexpectedByte(0))?;
        let bytes =
            serde_json::to_string(&self.inner).map_err(|_e| ed::Error::UnexpectedByte(0))?;
        let len = bytes.len() as u16;
        dest.write_all(&len.encode()?)?;
        dest.write_all(bytes.as_bytes())?;

        Ok(())
    }
}

impl<T> Decode for Adapter<T>
where
    T: for<'de> Deserialize<'de>,
    T: std::fmt::Debug,
{
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let len = u16::decode(&mut reader)?;
        let mut bytes = vec![0u8; len as usize];
        reader.read_exact(&mut bytes)?;
        let inner: T =
            serde_json::from_slice(&bytes).map_err(|_e| ed::Error::UnexpectedByte(124))?;
        Ok(Self { inner })
    }
}

impl<T> Terminated for Adapter<T> {}

impl<T> Describe for Adapter<T>
where
    for<'de> T: Deserialize<'de>,
    T: Serialize + std::fmt::Debug + 'static,
{
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

macro_rules! from_impl {
    ($type:ty) => {
        impl From<$type> for Adapter<$type> {
            fn from(inner: $type) -> Self {
                Self { inner }
            }
        }
    };
}
from_impl!(ClientId);
from_impl!((PortId, ChannelId));
from_impl!(ChannelId);
from_impl!(PortId);
from_impl!(ConnectionId);
from_impl!(Sequence);
from_impl!(ClientType);
from_impl!((ClientId, Height));
from_impl!(Height);
from_impl!(Timestamp);
from_impl!(PacketState);
from_impl!(Signer);
from_impl!(TimeoutHeight);

impl<T> Adapter<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
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

impl<T> State for Adapter<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
    T: std::fmt::Debug,
{
    fn attach(&mut self, _: Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> crate::Result<()> {
        Ok(())
    }
}

#[derive(Call, Query, Client, Default, Serialize, Deserialize)]
pub struct ProtobufAdapter<T> {
    inner: T,
}

impl<T> Encode for ProtobufAdapter<T>
where
    T: IbcProto,
    T: Protobuf<<T as IbcProto>::Proto>,
    <T as std::convert::TryFrom<<T as IbcProto>::Proto>>::Error: std::fmt::Display,
{
    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.inner.encoded_len())
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let mut bytes = vec![];
        self.inner
            .encode(&mut bytes)
            .map_err(|_e| ed::Error::UnexpectedByte(0))?;

        dest.write_all(&bytes)?;

        Ok(())
    }
}

impl<T> Decode for ProtobufAdapter<T>
where
    T: IbcProto,
    T: Protobuf<<T as IbcProto>::Proto>,
    <T as std::convert::TryFrom<<T as IbcProto>::Proto>>::Error: std::fmt::Display,
{
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        let inner: T = T::decode(bytes.as_slice()).map_err(|_e| ed::Error::UnexpectedByte(0))?;

        Ok(Self { inner })
    }
}

impl<T: 'static> Describe for ProtobufAdapter<T>
where
    T: IbcProto,
    T: Protobuf<<T as IbcProto>::Proto>,
    <T as std::convert::TryFrom<<T as IbcProto>::Proto>>::Error: std::fmt::Display,
{
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

use ibc_proto::google::protobuf::Any;
trait IbcProto: Sized {
    type Proto: From<Self> + Default + Message;
}

impl IbcProto for ibc::core::ics02_client::client_consensus::AnyConsensusState {
    type Proto = Any;
}
impl IbcProto for ibc::core::ics02_client::client_state::AnyClientState {
    type Proto = Any;
}
impl IbcProto for ibc::core::ics03_connection::connection::ConnectionEnd {
    type Proto = ibc_proto::ibc::core::connection::v1::ConnectionEnd;
}

impl IbcProto for ibc::clients::ics07_tendermint::consensus_state::ConsensusState {
    type Proto = ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState;
}

impl IbcProto for ibc::core::ics04_channel::channel::ChannelEnd {
    type Proto = ibc_proto::ibc::core::channel::v1::Channel;
}

impl<T> From<T> for ProtobufAdapter<T>
where
    T: IbcProto,
{
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
    T: IbcProto,
    T: Protobuf<<T as IbcProto>::Proto>,
    <T as std::convert::TryFrom<<T as IbcProto>::Proto>>::Error: std::fmt::Display,
{
    fn attach(&mut self, _: Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> crate::Result<()> {
        Ok(())
    }
}

impl Next for Adapter<ConnectionId> {
    fn next(&self) -> Option<Self> {
        let current_id: u64 = self.inner.as_str()[11..].parse().unwrap();
        if current_id == u64::MAX {
            return None;
        }
        Some(Self {
            inner: ConnectionId::new(current_id + 1),
        })
    }
}
