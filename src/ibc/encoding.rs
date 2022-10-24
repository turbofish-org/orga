use crate::call::Call;
use crate::client::Client;
use crate::collections::Next;
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
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, Call, Client, Query, Debug)]
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
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> crate::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> crate::Result<Self::Encoding> {
        Ok(self)
    }
}

#[derive(Call, Query, Client)]
pub struct ProtobufAdapter<T> {
    inner: T,
}

macro_rules! proto_adapter {
    ($type:ty, $proto:ty) => {
        impl Encode for ProtobufAdapter<$type> {
            fn encoding_length(&self) -> ed::Result<usize> {
                Ok(<$type as Protobuf<$proto>>::encoded_len(&self.inner))
            }

            fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
                let mut bytes = vec![];
                <$type as Protobuf<$proto>>::encode(&self.inner, &mut bytes)
                    .map_err(|_e| ed::Error::UnexpectedByte(0))?;

                dest.write_all(&bytes)?;

                Ok(())
            }
        }

        impl Decode for ProtobufAdapter<$type> {
            fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
                let mut bytes = vec![];
                reader.read_to_end(&mut bytes)?;

                let inner: $type = <$type as Protobuf<$proto>>::decode(bytes.as_slice())
                    .map_err(|_e| ed::Error::UnexpectedByte(0))?;

                Ok(Self { inner })
            }
        }

        impl State for ProtobufAdapter<$type> {
            type Encoding = Self;

            fn create(_: Store, data: Self::Encoding) -> crate::Result<Self> {
                Ok(data)
            }

            fn flush(self) -> crate::Result<Self::Encoding> {
                Ok(self)
            }
        }

        impl From<$type> for ProtobufAdapter<$type> {
            fn from(inner: $type) -> Self {
                Self { inner }
            }
        }
    };
}

proto_adapter!(
    ibc::clients::ics07_tendermint::consensus_state::ConsensusState,
    ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState
);
proto_adapter!(
    ibc::core::ics03_connection::connection::ConnectionEnd,
    ibc_proto::ibc::core::connection::v1::ConnectionEnd
);
proto_adapter!(
    ibc::core::ics04_channel::channel::ChannelEnd,
    ibc_proto::ibc::core::channel::v1::Channel
);
proto_adapter!(
    ibc::clients::ics07_tendermint::client_state::ClientState,
    ibc_proto::ibc::lightclients::tendermint::v1::ClientState
);

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
