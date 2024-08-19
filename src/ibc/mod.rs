use crate::migrate::MigrateFrom;
use ed::Terminated;
use ibc::apps::transfer::handler::send_transfer;
use ibc::clients::tendermint::{
    client_state::ClientState as TmClientState, consensus_state::ConsensusState as TmConsensusState,
};
use ibc::core::channel::types::channel::ChannelEnd as IbcChannelEnd;
use ibc::core::client::context::consensus_state::ConsensusState as ConsensusStateTrait;
use ibc::core::client::types::error::ClientError;
use ibc::primitives::proto::Any;
use ibc::primitives::Signer as IbcSigner;
use ibc_proto::ibc::applications::transfer::v1::MsgTransfer as RawMsgTransfer;
use ibc_proto::ibc::core::{
    channel::v1::Channel as RawChannelEnd, connection::v1::ConnectionEnd as RawConnectionEnd,
};
use ibc_proto::ibc::lightclients::tendermint::v1::Header as RawHeader;

use ibc::clients::tendermint::types::{
    TENDERMINT_CLIENT_STATE_TYPE_URL, TENDERMINT_CONSENSUS_STATE_TYPE_URL,
};
use ibc::primitives::proto::Protobuf;
use ibc_rs::apps::transfer::types::msgs::transfer::MsgTransfer;
use ibc_rs::clients::tendermint::types::{client_type, Header};
use ibc_rs::core::client::context::types::msgs::ClientMsg;
use ibc_rs::core::client::types::Height;
use ibc_rs::core::connection::types::ConnectionEnd;
use ibc_rs::core::entrypoint::dispatch;
use ibc_rs::core::handler::types::msgs::MsgEnvelope;
use ibc_rs::core::host::types::identifiers::{
    ChannelId, ClientId, ClientType, ConnectionId, PortId, Sequence,
};
use ibc_rs::core::host::types::path::{
    AckPath, ChannelEndPath, ClientConnectionPath, CommitmentPath, ConnectionPath, ReceiptPath,
    SeqAckPath, SeqRecvPath, SeqSendPath,
};
use serde::Serialize;

use crate::coins::Address;
use crate::collections::{Deque, Map};
use crate::context::GetContext;
use crate::describe::{Describe, Descriptor};
use crate::encoding::{
    Adapter, ByteTerminatedString, Decode, Encode, EofTerminatedString, FixedString,
};
use crate::migrate::{Migrate, MigrateInto};
use crate::plugins::Signer;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{orga, Error, Result as OrgaResult};
pub use ibc as ibc_rs;
use ibc::primitives::Timestamp as IbcTimestamp;

mod impls;
pub mod transfer;
use transfer::{Transfer, TransferInfo};
#[cfg(feature = "abci")]
mod service;
#[cfg(feature = "abci")]
pub use service::{start_grpc, GrpcOpts};

pub use self::messages::{IbcMessage, IbcTx, RawIbcTx};
mod client_contexts;
mod messages;
mod migration;
mod query;
mod router;

pub const IBC_QUERY_PATH: &str = "store/ibc/key";

#[orga(version = 3)]
pub struct Ibc {
    #[orga(version(V2))]
    _bytes1: [u8; 32],

    #[orga(version(V2))]
    _bytes2: [u8; 20],

    #[orga(version(V2))]
    #[state(absolute_prefix(b""))]
    root_store: Store,

    #[orga(version(V2))]
    #[state(prefix(b""))]
    local_store: Store,

    #[orga(version(V3))]
    pub ctx: IbcContext,

    #[orga(version(V3))]
    pub router: router::IbcRouter,
}

#[orga(version = 1)]
pub struct IbcContext {
    height: u64,
    host_consensus_states: Deque<WrappedConsensusState>,
    channel_counter: u64,
    connection_counter: u64,
    client_counter: u64,

    #[state(absolute_prefix(b"clients/"))]
    pub clients: Map<ClientIdKey, Client>,

    #[state(absolute_prefix(b"connections/"))]
    pub connections: Map<ConnectionIdKey, WrappedConnectionEnd>,

    #[state(absolute_prefix(b"channelEnds/"))]
    pub channel_ends: Map<PortChannel, WrappedChannelEnd>,

    #[state(absolute_prefix(b"nextSequenceSend/"))]
    pub next_sequence_send: Map<PortChannel, Number>,

    #[state(absolute_prefix(b"nextSequenceRecv/"))]
    pub next_sequence_recv: Map<PortChannel, Number>,

    #[state(absolute_prefix(b"nextSequenceAck/"))]
    pub next_sequence_ack: Map<PortChannel, Number>,

    #[state(absolute_prefix(b"commitments/"))]
    pub commitments: Map<PortChannelSequence, Vec<u8>>,

    #[orga(version(V0))]
    #[state(absolute_prefix(b"receipts/"))]
    pub receipts: Map<PortChannelSequence, ()>,

    #[orga(version(V1))]
    #[state(absolute_prefix(b"receipts/"))]
    pub receipts: Map<PortChannelSequence, u8>,

    #[state(absolute_prefix(b"acks/"))]
    pub acks: Map<PortChannelSequence, Vec<u8>>,

    #[state(absolute_prefix(b""))]
    pub store: Store,
}

#[orga]
impl Ibc {
    pub fn deliver(&mut self, messages: RawIbcTx) -> crate::Result<Vec<TransferInfo>> {
        let messages: IbcTx = messages.try_into()?;
        let mut incoming_transfers = vec![];
        for message in messages.0 {
            if let Some(incoming_transfer) = self.deliver_message(message)? {
                incoming_transfers.push(incoming_transfer);
            }
        }
        Ok(incoming_transfers)
    }

    #[call]
    pub fn raw_transfer(&mut self, message: TransferMessage) -> crate::Result<()> {
        let message: MsgTransfer = message.inner;
        let sender_addr: Address = message.packet_data.sender.clone().try_into()?;

        if self.signer()? != sender_addr {
            return Err(crate::Error::Ibc(
                "Transfers must be signed by the sender".into(),
            ));
        }

        self.deliver_message(IbcMessage::Ics20(message))?;

        Ok(())
    }

    pub fn deliver_message(&mut self, message: IbcMessage) -> crate::Result<Option<TransferInfo>> {
        let mut maybe_client_update = None;

        use IbcMessage::*;

        match message {
            Ics26(msg) => {
                if let MsgEnvelope::Client(ClientMsg::UpdateClient(msg)) = &msg {
                    maybe_client_update = Some(msg.clone());
                }
                dispatch(&mut self.ctx, &mut self.router, msg)
                    .map_err(|e| Error::Ibc(e.to_string()))?
            }
            Ics20(msg) => {
                let transfer_module = &mut self.router.transfer;
                send_transfer(&mut self.ctx, transfer_module, msg)
                    .map_err(|e| Error::Ibc(e.to_string()))?
            }
        };

        if let Some(msg) = maybe_client_update {
            match Header::try_from(msg.client_message) {
                Ok(header) => {
                    let mut client = self
                        .ctx
                        .clients
                        .get_mut(msg.client_id.into())?
                        .ok_or_else(|| Error::Ibc("Expected client".to_string()))?;
                    client.last_header = Some(header.into())
                }
                Err(err) => log::debug!("Error decoding header: {}", err),
            }
        }

        Ok(self.transfer_mut().incoming_transfer_mut().take())
    }

    fn signer(&mut self) -> crate::Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Coins("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Call must be signed".into()))
    }

    pub fn transfer(&self) -> &Transfer {
        &self.router.transfer
    }

    pub fn transfer_mut(&mut self) -> &mut Transfer {
        &mut self.router.transfer
    }

    pub fn update_client_from_header(
        &mut self,
        _client_index: u64,
        _rev_number: u64,
        _header_json: &str,
    ) -> crate::Result<()> {
        todo!();
    }
}

impl std::fmt::Debug for IbcContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IbcContext").finish()
    }
}

#[orga]
pub struct Client {
    #[state(prefix(b"updates/"))]
    pub updates: Map<EpochHeight, (WrappedTimestamp, BinaryHeight)>,

    #[state(prefix(b"client"))]
    pub client_state: Map<FixedString<"State">, WrappedClientState>,

    #[state(prefix(b"consensusStates/"))]
    pub consensus_states: Map<EpochHeight, WrappedConsensusState>,

    #[state(prefix(b"connections/"))]
    pub connections: Map<ConnectionIdKey, ()>,

    // TODO: support headers for non-tendermint clients
    pub last_header: Option<WrappedHeader>,

    client_type: EofTerminatedString,
}

impl Client {
    pub fn client_type(&self) -> crate::Result<ClientType> {
        Ok(client_type())
    }

    pub fn set_client_type(&mut self, client_type: ClientType) {
        self.client_type = client_type.into();
    }

    pub fn last_header(&self) -> crate::Result<Header> {
        Ok(self
            .last_header
            .as_ref()
            .ok_or_else(|| Error::Ibc("Expected relayed header".to_string()))?
            .clone()
            .into())
    }
}

pub type SlashTerminatedString<T> = ByteTerminatedString<b'/', T>;

pub type ClientIdKey = SlashTerminatedString<ClientId>;
pub type ConnectionIdKey = EofTerminatedString<ConnectionId>;
pub type Number = EofTerminatedString<u64>;
pub type EpochHeight = EofTerminatedString;

#[orga(simple, skip(Migrate, Default))]
#[derive(Debug)]
pub struct WrappedTimestamp {
    inner: IbcTimestamp,
}

impl Migrate for WrappedTimestamp {}

impl Describe for WrappedTimestamp {
    fn describe() -> Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl Encode for Adapter<WrappedTimestamp> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        self.0.inner.nanoseconds().encode_into(dest)
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        self.0.inner.nanoseconds().encoding_length()
    }
}

impl Decode for Adapter<WrappedTimestamp> {
    fn decode<R: std::io::Read>(input: R) -> ed::Result<Self> {
        Ok(Self(WrappedTimestamp {
            inner: IbcTimestamp::from_nanoseconds(u64::decode(input)?),
        }))
    }
}

impl From<WrappedTimestamp> for IbcTimestamp {
    fn from(timestamp: WrappedTimestamp) -> Self {
        timestamp.inner
    }
}

impl From<IbcTimestamp> for WrappedTimestamp {
    fn from(timestamp: IbcTimestamp) -> Self {
        Self { inner: timestamp }
    }
}

impl Encode for Adapter<IbcSigner> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        borsh::BorshSerialize::serialize(&self.0, dest).map_err(|_| ed::Error::UnexpectedByte(40))
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        let mut buf = vec![];
        borsh::BorshSerialize::serialize(&self.0, &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(40))?;
        Ok(buf.len())
    }
}

impl Decode for Adapter<IbcSigner> {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut len_buf = [0; 4];
        input.read_exact(&mut len_buf)?;
        let len: u32 = borsh::BorshDeserialize::deserialize(&mut len_buf.as_slice())
            .map_err(|_| ed::Error::UnexpectedByte(40))?;

        let mut buf = vec![0; len as usize];

        input.read_exact(&mut buf)?;
        let bytes = [len_buf.to_vec(), buf].concat();

        Ok(Self(
            borsh::BorshDeserialize::deserialize(&mut bytes.as_slice())
                .map_err(|_| ed::Error::UnexpectedByte(40))?,
        ))
    }
}

impl Terminated for Adapter<IbcSigner> {}

#[orga]
#[derive(Clone, Debug)]
pub struct BinaryHeight {
    epoch: u64,
    height: u64,
}

impl From<Height> for BinaryHeight {
    fn from(height: Height) -> Self {
        Self {
            epoch: height.revision_number(),
            height: height.revision_height(),
        }
    }
}

impl TryFrom<BinaryHeight> for Height {
    type Error = ();

    fn try_from(height: BinaryHeight) -> Result<Self, Self::Error> {
        Height::new(height.epoch, height.height).map_err(|_| ())
    }
}

impl Terminated for Adapter<WrappedTimestamp> {}

impl From<ConnectionPath> for ConnectionIdKey {
    fn from(path: ConnectionPath) -> Self {
        Self(path.0)
    }
}

impl From<ClientConnectionPath> for ClientIdKey {
    fn from(path: ClientConnectionPath) -> Self {
        Self(path.0)
    }
}

impl From<Sequence> for Number {
    fn from(sequence: Sequence) -> Self {
        Self(sequence.into())
    }
}

impl From<Height> for EpochHeight {
    fn from(height: Height) -> Self {
        Self(format!(
            "{}-{}",
            height.revision_number(),
            height.revision_height()
        ))
    }
}

impl TryFrom<EpochHeight> for Height {
    type Error = Error;

    fn try_from(epoch_height: EpochHeight) -> Result<Self, Self::Error> {
        let mut parts = epoch_height.0.split('-');
        let revision_number = parts
            .next()
            .ok_or(Error::Ibc("Invalid revision number".to_string()))?
            .parse()
            .map_err(|_| Error::Ibc("Invalid revision number".to_string()))?;
        let revision_height = parts
            .next()
            .ok_or(Error::Ibc("Invalid revision height".to_string()))?
            .parse()
            .map_err(|_| Error::Ibc("Invalid revision height".to_string()))?;
        Height::new(revision_number, revision_height)
            .map_err(|_| Error::Ibc("Failed to parse height".to_string()))
    }
}

impl From<ClientType> for EofTerminatedString {
    fn from(client_type: ClientType) -> Self {
        Self(client_type.as_str().to_string())
    }
}

impl From<EofTerminatedString> for ClientType {
    fn from(client_type: EofTerminatedString) -> Self {
        ClientType::new(client_type.0.as_str()).unwrap()
    }
}

#[derive(State, Encode, Decode, Serialize, Clone, Debug)]
pub struct PortChannel(
    #[serde(skip)] FixedString<"ports/">,
    SlashTerminatedString<PortId>,
    #[serde(skip)] FixedString<"channels/">,
    EofTerminatedString<ChannelId>,
);

impl Migrate for PortChannel {
    fn migrate(_src: Store, _dest: Store, bytes: &mut &[u8]) -> OrgaResult<Self> {
        Ok(Decode::decode(bytes)?)
    }
}

impl Describe for PortChannel {
    fn describe() -> Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl PortChannel {
    pub fn new(port_id: PortId, channel_id: ChannelId) -> Self {
        Self(
            FixedString,
            ByteTerminatedString(port_id),
            FixedString,
            EofTerminatedString(channel_id),
        )
    }

    pub fn port_id(&self) -> crate::Result<PortId> {
        self.1
            .clone()
            .to_string()
            .parse()
            .map_err(|_| Error::Ibc("Invalid port ID".to_string()))
    }

    pub fn channel_id(&self) -> crate::Result<ChannelId> {
        self.3
            .clone()
            .to_string()
            .parse()
            .map_err(|_| Error::Ibc("Invalid channel ID".to_string()))
    }

    pub fn with_sequence(self, sequence: Sequence) -> crate::Result<PortChannelSequence> {
        Ok(PortChannelSequence::new(
            self.port_id()?,
            self.channel_id()?,
            sequence,
        ))
    }
}

macro_rules! port_channel_from_impl {
    ($ty:ty) => {
        impl From<$ty> for PortChannel {
            fn from(path: $ty) -> Self {
                Self(
                    FixedString,
                    ByteTerminatedString(path.0),
                    FixedString,
                    EofTerminatedString(path.1),
                )
            }
        }
    };
}

port_channel_from_impl!(ChannelEndPath);
port_channel_from_impl!(SeqSendPath);
port_channel_from_impl!(SeqRecvPath);
port_channel_from_impl!(SeqAckPath);

#[derive(State, Encode, Decode, Serialize, Clone, Debug)]
pub struct PortChannelSequence(
    #[serde(skip)] FixedString<"ports/">,
    SlashTerminatedString<PortId>,
    #[serde(skip)] FixedString<"channels/">,
    SlashTerminatedString<ChannelId>,
    #[serde(skip)] FixedString<"sequences/">,
    EofTerminatedString<Sequence>,
);

impl Migrate for PortChannelSequence {
    fn migrate(_src: Store, _dest: Store, bytes: &mut &[u8]) -> OrgaResult<Self> {
        Ok(Decode::decode(bytes)?)
    }
}

impl Describe for PortChannelSequence {
    fn describe() -> Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl PortChannelSequence {
    pub fn new(port_id: PortId, channel_id: ChannelId, sequence: Sequence) -> Self {
        Self(
            FixedString,
            ByteTerminatedString(port_id),
            FixedString,
            ByteTerminatedString(channel_id),
            FixedString,
            EofTerminatedString(sequence),
        )
    }

    pub fn port_id(&self) -> crate::Result<PortId> {
        self.1
            .clone()
            .to_string()
            .parse()
            .map_err(|_| Error::Ibc("Invalid port ID".to_string()))
    }

    pub fn channel_id(&self) -> crate::Result<ChannelId> {
        self.3
            .clone()
            .to_string()
            .parse()
            .map_err(|_| Error::Ibc("Invalid channel ID".to_string()))
    }

    pub fn sequence(&self) -> crate::Result<Sequence> {
        self.5
            .clone()
            .to_string()
            .parse()
            .map_err(|_| Error::Ibc("Invalid sequence".to_string()))
    }
}

macro_rules! port_channel_sequence_from_impl {
    ($ty:ty) => {
        impl From<$ty> for PortChannelSequence {
            fn from(path: $ty) -> Self {
                Self(
                    FixedString,
                    ByteTerminatedString(path.port_id),
                    FixedString,
                    ByteTerminatedString(path.channel_id),
                    FixedString,
                    EofTerminatedString(path.sequence),
                )
            }
        }
    };
}

port_channel_sequence_from_impl!(CommitmentPath);
port_channel_sequence_from_impl!(AckPath);
port_channel_sequence_from_impl!(ReceiptPath);

macro_rules! protobuf_newtype {
    ($newtype:tt, $inner:ty, $raw:ty, $proto:tt, $prev:ty) => {
        #[derive(Serialize, Clone, Debug)]
        pub struct $newtype {
            pub inner: $inner,
        }

        impl Encode for $newtype {
            fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
                let mut buf = vec![];
                $proto::<$raw>::encode(self.inner.clone(), &mut buf)
                    .map_err(|_| ed::Error::UnexpectedByte(10))?;
                dest.write_all(&buf)?;
                Ok(())
            }

            fn encoding_length(&self) -> ed::Result<usize> {
                let mut buf = vec![];
                $proto::<$raw>::encode(self.inner.clone(), &mut buf)
                    .map_err(|_| ed::Error::UnexpectedByte(10))?;
                Ok(buf.len())
            }
        }

        impl Decode for $newtype {
            fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
                let mut buf = vec![];
                input.read_to_end(&mut buf)?;
                let inner = $proto::<$raw>::decode(buf.as_slice())
                    .map_err(|_| ed::Error::UnexpectedByte(10))?;
                Ok(Self { inner })
            }
        }

        impl crate::state::State for $newtype {
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

        impl From<$inner> for $newtype {
            fn from(inner: $inner) -> Self {
                Self { inner }
            }
        }

        impl From<$newtype> for $inner {
            fn from(outer: $newtype) -> Self {
                outer.inner
            }
        }

        #[allow(trivial_bounds)]
        impl From<$newtype> for $raw
        where
            $inner: Into<$raw>,
        {
            fn from(outer: $newtype) -> Self {
                outer.inner.into()
            }
        }

        impl Migrate for $newtype {
            fn migrate(_src: Store, _dest: Store, bytes: &mut &[u8]) -> OrgaResult<Self> {
                let prev = <$prev>::load(Store::default(), bytes)?;
                prev.migrate_into()
            }
        }

        impl Describe for $newtype {
            fn describe() -> Descriptor {
                crate::describe::Builder::new::<Self>().build()
            }
        }

        impl Query for $newtype {
            type Query = ();

            fn query(&self, _: ()) -> crate::Result<()> {
                Ok(())
            }
        }
    };
}
use ibc::primitives::proto::Any as IbcAny;

protobuf_newtype!(
    WrappedClientState,
    TmClientState,
    IbcAny,
    Protobuf,
    WrappedClientState
);

// protobuf_newtype!(
//     ConsensusState,
//     TmConsensusState,
//     ibc::Any,
//     Protobuf,
//     ConsensusState
// );

pub struct LegacyWrappedConnectionEnd {
    pub inner: ConnectionEnd,
}

impl State for LegacyWrappedConnectionEnd {
    fn attach(&mut self, _store: crate::store::Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, _out: &mut W) -> crate::Result<()> {
        Ok(())
    }

    fn load(_store: crate::store::Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        let mut inner: RawConnectionEnd =
            <RawConnectionEnd as prost::Message>::decode(bytes).unwrap();
        inner.versions = ibc::core::connection::types::version::Version::compatibles()
            .into_iter()
            .map(|v| v.into())
            .collect();
        Ok(Self {
            inner: inner.try_into().map_err(|_| {
                crate::Error::Ibc("Unable to load state for LegacyWrappedConnectionEnd".to_string())
            })?,
        })
    }
}

impl MigrateFrom<LegacyWrappedConnectionEnd> for WrappedConnectionEnd {
    fn migrate_from(value: LegacyWrappedConnectionEnd) -> crate::Result<Self> {
        Ok(Self { inner: value.inner })
    }
}

protobuf_newtype!(
    WrappedConnectionEnd,
    ConnectionEnd,
    RawConnectionEnd,
    Protobuf,
    LegacyWrappedConnectionEnd
);
protobuf_newtype!(
    WrappedChannelEnd,
    IbcChannelEnd,
    RawChannelEnd,
    Protobuf,
    WrappedChannelEnd
);

protobuf_newtype!(WrappedHeader, Header, RawHeader, Protobuf, WrappedHeader);
impl Terminated for WrappedHeader {}

#[derive(Serialize, Clone, Debug)]
pub struct WrappedConsensusState {
    inner: TmConsensusState,
}

impl Encode for WrappedConsensusState {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let mut buf = vec![];
        Protobuf::<IbcAny>::encode(self.inner.clone(), &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        dest.write_all(&buf)?;
        Ok(())
    }
    fn encoding_length(&self) -> ed::Result<usize> {
        let mut buf = vec![];
        Protobuf::<IbcAny>::encode(self.inner.clone(), &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        Ok(buf.len())
    }
}
impl Decode for WrappedConsensusState {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut buf = vec![];
        input.read_to_end(&mut buf)?;
        let inner = Protobuf::<IbcAny>::decode(buf.as_slice())
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        Ok(Self { inner })
    }
}
impl crate::state::State for WrappedConsensusState {
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

impl From<TmConsensusState> for WrappedConsensusState {
    fn from(inner: TmConsensusState) -> Self {
        Self { inner }
    }
}

impl TryFrom<WrappedConsensusState> for TmConsensusState {
    type Error = &'static str;

    fn try_from(outer: WrappedConsensusState) -> Result<Self, &'static str> {
        Ok(outer.inner)
    }
}
#[allow(trivial_bounds)]
impl From<WrappedConsensusState> for IbcAny
where
    TmConsensusState: Into<IbcAny>,
{
    fn from(outer: WrappedConsensusState) -> Self {
        outer.inner.into()
    }
}

impl Migrate for WrappedConsensusState {
    fn migrate(_src: Store, _dest: Store, bytes: &mut &[u8]) -> OrgaResult<Self> {
        let prev = <WrappedConsensusState>::load(Store::default(), bytes)?;
        prev.migrate_into()
    }
}

impl Describe for WrappedConsensusState {
    fn describe() -> Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl TryFrom<Any> for WrappedConsensusState {
    type Error = ClientError;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        match value.type_url.as_str() {
            TENDERMINT_CONSENSUS_STATE_TYPE_URL => {
                let inner = TmConsensusState::try_from(value)?;
                Ok(Self { inner })
            }
            _ => Err(ClientError::Other {
                description: "Unknown consensus state type".into(),
            }),
        }
    }
}

impl ConsensusStateTrait for WrappedConsensusState {
    fn root(&self) -> &ibc_rs::core::commitment_types::commitment::CommitmentRoot {
        self.inner.root()
    }

    fn timestamp(&self) -> IbcTimestamp {
        self.inner
            .timestamp()
            .try_into()
            .expect("UNIX Timestamp can't be negative")
    }
}

#[derive(Debug, Clone)]
pub struct TransferMessage {
    inner: MsgTransfer,
}

impl From<MsgTransfer> for TransferMessage {
    fn from(inner: MsgTransfer) -> Self {
        Self { inner }
    }
}

impl Encode for TransferMessage {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let mut buf = vec![];
        Protobuf::<RawMsgTransfer>::encode(self.inner.clone(), &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        dest.write_all(&buf)?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        let mut buf = vec![];
        Protobuf::<RawMsgTransfer>::encode(self.inner.clone(), &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        Ok(buf.len())
    }
}

impl Decode for TransferMessage {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut buf = vec![];
        input.read_to_end(&mut buf)?;
        let inner = Protobuf::<RawMsgTransfer>::decode(buf.as_slice())
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        Ok(Self { inner })
    }
}

impl TryFrom<IbcSigner> for Address {
    type Error = crate::Error;

    fn try_from(signer: IbcSigner) -> crate::Result<Self> {
        signer
            .as_ref()
            .parse()
            .map_err(|_| crate::Error::Ibc("Invalid signer".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use ibc_rs::{
        clients::tendermint::types::{AllowUpdate, TrustThreshold},
        core::{
            commitment_types::{commitment::CommitmentRoot, specs::ProofSpecs},
            host::types::identifiers::ChainId,
        },
    };
    use tendermint::{Hash, Time};

    use super::*;
    use crate::{
        state::State,
        store::{BackingStore, MapStore, Read, Shared, Store, Write},
    };

    #[orga]
    pub struct App {
        ibc: Ibc,
    }

    fn create_state() -> Store {
        let mut store = Store::new(BackingStore::MapStore(Shared::new(MapStore::new())));

        let mut app = App::default();
        app.attach(store.clone()).unwrap();
        let ibc = &mut app.ibc;

        ibc.ctx.channel_counter = 123;
        ibc.ctx.connection_counter = 456;
        ibc.ctx.client_counter = 789;

        let mut client = Client::default();
        let client_state = TmClientState::from(ibc::clients::tendermint::types::ClientState {
            chain_id: ChainId::new("foo-0").unwrap(),
            trust_level: TrustThreshold::ONE_THIRD,
            trusting_period: Duration::from_secs(60 * 60 * 24 * 7),
            unbonding_period: Duration::from_secs(60 * 60 * 24 * 14),
            max_clock_drift: Duration::from_secs(60),
            latest_height: Height::new(0, 1234).unwrap(),
            proof_specs: ProofSpecs::cosmos(),
            upgrade_path: vec![],
            allow_update: AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
            frozen_height: None,
        })
        .into();
        client
            .client_state
            .insert(Default::default(), client_state)
            .unwrap();
        let consensus_state =
            TmConsensusState::from(ibc::clients::tendermint::types::ConsensusState {
                root: CommitmentRoot::from_bytes(&[0; 32]),
                timestamp: Time::from_unix_timestamp(0, 0).unwrap(),
                next_validators_hash: Hash::Sha256([5; 32]),
            })
            .into();
        client
            .consensus_states
            .insert("0-100".to_string().into(), consensus_state)
            .unwrap();
        let client_id = ClientId::new("07-tendermint", 123).unwrap();
        client.client_type = ClientType::new("07-tendermint").unwrap().into();
        client
            .updates
            .insert(
                "0-100".to_string().into(),
                (
                    IbcTimestamp::from_nanoseconds(0).into(),
                    Height::new(0, 123).unwrap().into(),
                ),
            )
            .unwrap();
        let conn_id = ConnectionId::new(123);
        client
            .connections
            .insert(conn_id.clone().into(), ())
            .unwrap();

        ibc.ctx
            .clients
            .insert(client_id.clone().into(), client)
            .unwrap();
        let counterparty = ibc::core::connection::types::Counterparty::new(
            ClientId::new("07-tendermint", 0).unwrap(),
            None,
            vec![1, 2, 3].try_into().unwrap(),
        );
        let conn = ConnectionEnd::new(
            ibc::core::connection::types::State::Uninitialized,
            ClientId::new("07-tendermint", 0).unwrap(),
            counterparty,
            ibc::core::connection::types::version::Version::compatibles(),
            Default::default(),
        )
        .unwrap();
        ibc.ctx
            .connections
            .insert(conn_id.into(), conn.into())
            .unwrap();

        let channel_end_path = ChannelEndPath(PortId::transfer(), ChannelId::new(123)).into();
        let counterparty = ibc::core::channel::types::channel::Counterparty::new(
            ibc::core::host::types::identifiers::PortId::new("defaultPort".to_string()).unwrap(),
            None,
        );
        let chan = ibc::core::channel::types::channel::ChannelEnd::new(
            ibc::core::channel::types::channel::State::Open,
            ibc::core::channel::types::channel::Order::Unordered,
            counterparty,
            Default::default(),
            ibc::core::channel::types::Version::new("".to_string()),
        )
        .unwrap()
        .into();
        ibc.ctx.channel_ends.insert(channel_end_path, chan).unwrap();

        let seq_sends_path = SeqSendPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.ctx
            .next_sequence_send
            .insert(seq_sends_path, 1.into())
            .unwrap();

        let seq_recvs_path = SeqRecvPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.ctx
            .next_sequence_recv
            .insert(seq_recvs_path, 2.into())
            .unwrap();

        let seq_acks_path = SeqAckPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.ctx
            .next_sequence_ack
            .insert(seq_acks_path, 3.into())
            .unwrap();

        let commitments_path = CommitmentPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.ctx
            .commitments
            .insert(commitments_path, vec![1, 2, 3])
            .unwrap();

        let acks_path = AckPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.ctx.acks.insert(acks_path, vec![1, 2, 3]).unwrap();

        let receipts_path = ReceiptPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.ctx.receipts.insert(receipts_path, 1).unwrap();

        let mut bytes = vec![];
        app.flush(&mut bytes).unwrap();

        store.put(vec![], bytes).unwrap();

        store
    }

    fn assert_state(store: &Store) {
        let mut entries = store.range(..);

        let mut assert_next = |key: &[u8], value: &[u8]| {
            let (k, v) = entries.next().unwrap().unwrap();
            assert_eq!(
                String::from_utf8(key.to_vec()).unwrap(),
                String::from_utf8(k).unwrap(),
            );
            assert_eq!(
                v,
                value,
                "key: {}",
                String::from_utf8(key.to_vec()).unwrap()
            );
        };

        assert_next(
            &[],
            &[
                0, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 127, 255, 255, 255, 255, 255, 255, 255, 127,
                255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 123, 0, 0, 0, 0, 0, 0, 1,
                200, 0, 0, 0, 0, 0, 0, 3, 21, 0, 0,
            ],
        );

        assert_next(
            b"acks/ports/transfer/channels/channel-123/sequences/1",
            &[1, 2, 3],
        );
        assert_next(
            b"channelEnds/ports/transfer/channels/channel-123",
            &[
                8, 3, 16, 1, 26, 13, 10, 11, 100, 101, 102, 97, 117, 108, 116, 80, 111, 114, 116,
            ],
        );
        assert_next(b"clients/07-tendermint-123/", b"\x00\x0007-tendermint");
        assert_next(
            b"clients/07-tendermint-123/clientState",
            &[
                10, 43, 47, 105, 98, 99, 46, 108, 105, 103, 104, 116, 99, 108, 105, 101, 110, 116,
                115, 46, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 46, 118, 49, 46, 67,
                108, 105, 101, 110, 116, 83, 116, 97, 116, 101, 18, 90, 10, 5, 102, 111, 111, 45,
                48, 18, 4, 8, 1, 16, 3, 26, 4, 8, 128, 245, 36, 34, 4, 8, 128, 234, 73, 42, 2, 8,
                60, 50, 0, 58, 3, 16, 210, 9, 66, 25, 10, 9, 8, 1, 24, 1, 32, 1, 42, 1, 0, 18, 12,
                10, 2, 0, 1, 16, 33, 24, 4, 32, 12, 48, 1, 66, 25, 10, 9, 8, 1, 24, 1, 32, 1, 42,
                1, 0, 18, 12, 10, 2, 0, 1, 16, 32, 24, 1, 32, 1, 48, 1,
            ],
        );
        assert_next(b"clients/07-tendermint-123/connections/connection-123", &[]);
        assert_next(
            b"clients/07-tendermint-123/consensusStates/0-100",
            &[
                10, 46, 47, 105, 98, 99, 46, 108, 105, 103, 104, 116, 99, 108, 105, 101, 110, 116,
                115, 46, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 46, 118, 49, 46, 67,
                111, 110, 115, 101, 110, 115, 117, 115, 83, 116, 97, 116, 101, 18, 72, 10, 0, 18,
                34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 26, 32, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
                5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            ],
        );
        assert_next(
            b"clients/07-tendermint-123/updates/0-100",
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 123,
            ],
        );
        assert_next(
            b"commitments/ports/transfer/channels/channel-123/sequences/1",
            &[1, 2, 3],
        );
        assert_next(
            b"connections/connection-123",
            &[
                10, 15, 48, 55, 45, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 45, 48, 18,
                35, 10, 1, 49, 18, 13, 79, 82, 68, 69, 82, 95, 79, 82, 68, 69, 82, 69, 68, 18, 15,
                79, 82, 68, 69, 82, 95, 85, 78, 79, 82, 68, 69, 82, 69, 68, 34, 24, 10, 15, 48, 55,
                45, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 45, 48, 26, 5, 10, 3, 1, 2,
                3,
            ],
        );
        assert_next(b"nextSequenceAck/ports/transfer/channels/channel-123", b"3");
        assert_next(
            b"nextSequenceRecv/ports/transfer/channels/channel-123",
            b"2",
        );
        assert_next(
            b"nextSequenceSend/ports/transfer/channels/channel-123",
            b"1",
        );
        assert_next(
            b"receipts/ports/transfer/channels/channel-123/sequences/1",
            &[1],
        );
        assert!(entries.next().is_none());
    }

    fn create_ibc_state_from_file<I: Into<PathBuf>>(path: I) -> Store {
        use std::fs::File;
        use std::io::{self, BufRead};

        let file = File::open(&path.into()).unwrap();
        let reader = io::BufReader::new(file);

        let mut tuples = Vec::new();
        let mut lines = reader.lines();
        while let Some(Ok(line)) = lines.next() {
            let tuple: (Vec<u8>, Vec<u8>) = serde_json::from_str(&line).unwrap();
            tuples.push(tuple);
        }

        let mut store = Store::new(BackingStore::MapStore(Shared::new(MapStore::new())));
        for (key, value) in tuples {
            store.put(key, value).unwrap();
        }

        store
    }

    #[test]
    fn migrate_v8() {
        let mut store = create_ibc_state_from_file("v8-ibc-test-state.txt");

        let mut bytes = store.get(&[]).unwrap().unwrap();
        let app = App::migrate(store.clone(), store.clone(), &mut bytes.as_slice()).unwrap();
        bytes.clear();
        app.flush(&mut bytes).unwrap();
        store.put(vec![], bytes).unwrap();

        assert_state(&store);
    }

    #[test]
    fn state_structure() {
        let store = create_state();
        assert_state(&store);
    }

    #[test]
    fn migrate() {
        let mut store = create_state();

        let mut bytes = store.get(&[]).unwrap().unwrap();
        let app = App::migrate(store.clone(), store.clone(), &mut bytes.as_slice()).unwrap();
        bytes.clear();
        app.flush(&mut bytes).unwrap();
        store.put(vec![], bytes).unwrap();

        assert_state(&store);
    }
}
