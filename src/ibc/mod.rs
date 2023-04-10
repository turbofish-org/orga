use ibc::clients::ics07_tendermint::client_state::ClientState as TmClientState;
use ibc::core::ics04_channel::packet::Sequence;
use ibc::core::ics24_host::identifier::{
    ChannelId, ClientId as IbcClientId, ConnectionId as IbcConnectionId, PortId,
};
use ibc::core::ics24_host::path::{
    AckPath, ChannelEndPath, CommitmentPath, ReceiptPath, SeqAckPath, SeqRecvPath, SeqSendPath,
};
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::Serialize;

use crate::collections::Map;
use crate::encoding::{
    Adapter, ByteTerminatedString, Decode, Encode, EofTerminatedString, FixedString,
};
use crate::orga;

#[orga]
pub struct Ibc {
    #[state(absolute_prefix(b"clients/"))]
    clients: Map<ClientId, Client>,

    #[state(absolute_prefix(b"connections/"))]
    connections: Map<ConnectionId, Connection>,

    #[state(absolute_prefix(b"channelEnds/"))]
    channel_ends: Map<PortChannel, ChannelEnd>,

    #[state(absolute_prefix(b"nextSequenceSend/"))]
    next_sequence_send: Map<PortChannel, u64>,

    #[state(absolute_prefix(b"nextSequenceRecv/"))]
    next_sequence_recv: Map<PortChannel, u64>,

    #[state(absolute_prefix(b"nextSequenceAck/"))]
    next_sequence_ack: Map<PortChannel, u64>,

    #[state(absolute_prefix(b"commitments/"))]
    commitments: Map<PortChannelSequence, Vec<u8>>,

    #[state(absolute_prefix(b"receipts/"))]
    receipts: Map<PortChannelSequence, Vec<u8>>,

    #[state(absolute_prefix(b"acks/"))]
    acks: Map<PortChannelSequence, Vec<u8>>,
}

#[orga]
pub struct Client {
    #[state(prefix(b"clientState"))]
    client_state: Map<(), ClientState>,

    #[state(prefix(b"consensusStates/"))]
    consensus_states: Map<Height, ConsensusState>,
}

pub type SlashTerminatedString<T> = ByteTerminatedString<b'/', T>;

pub type ClientId = SlashTerminatedString<IbcClientId>;
pub type ConnectionId = EofTerminatedString<IbcConnectionId>;
pub type Height = EofTerminatedString<u64>;

#[derive(Encode, Decode, Serialize, Clone, Debug)]
pub struct PortChannel(
    #[serde(skip)] FixedString<"ports/">,
    SlashTerminatedString<PortId>,
    #[serde(skip)] FixedString<"channels/">,
    EofTerminatedString<ChannelId>,
);

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

#[derive(Encode, Decode, Serialize, Clone, Debug)]
pub struct PortChannelSequence(
    #[serde(skip)] FixedString<"ports/">,
    SlashTerminatedString<PortId>,
    #[serde(skip)] FixedString<"channels/">,
    SlashTerminatedString<ChannelId>,
    #[serde(skip)] FixedString<"sequences/">,
    EofTerminatedString<Sequence>,
);

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

#[orga(skip(Default), simple)]
pub struct ClientState {
    inner: TmClientState,
}

impl Encode for Adapter<ClientState> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let mut buf = vec![];
        Protobuf::<Any>::encode(&self.0.inner, &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        dest.write_all(&buf)?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        let mut buf = vec![];
        Protobuf::<Any>::encode(&self.0.inner, &mut buf)
            .map_err(|_| ed::Error::UnexpectedByte(10))?;
        Ok(buf.len())
    }
}

impl Decode for Adapter<ClientState> {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut buf = vec![];
        input.read_to_end(&mut buf)?;
        let inner =
            Protobuf::<Any>::decode(buf.as_slice()).map_err(|_| ed::Error::UnexpectedByte(10))?;
        Ok(Self(ClientState { inner }))
    }
}

impl From<TmClientState> for ClientState {
    fn from(inner: TmClientState) -> Self {
        Self { inner }
    }
}

pub type ConsensusState = ();
pub type Connection = ();
pub type ChannelEnd = ();

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ibc::{
        clients::ics07_tendermint::client_state::AllowUpdate,
        core::{
            ics02_client::{
                client_type::ClientType, height::Height, trust_threshold::TrustThreshold,
            },
            ics23_commitment::specs::ProofSpecs,
            ics24_host::identifier::ChainId,
        },
    };

    use super::*;
    use crate::{
        merk::BackingStore,
        state::State,
        store::{MapStore, Shared, Store},
    };

    #[orga]
    pub struct App {
        ibc: Ibc,
    }

    #[test]
    fn state_structure() {
        let store = Store::new(BackingStore::MapStore(Shared::new(MapStore::new())));

        let mut app = App::default();
        app.attach(store.clone()).unwrap();
        let ibc = &mut app.ibc;

        let mut client = Client::default();
        let client_state = TmClientState::new(
            ChainId::new("foo".to_string(), 0),
            TrustThreshold::default(),
            Duration::from_secs(60 * 60 * 24 * 7),
            Duration::from_secs(60 * 60 * 24 * 14),
            Duration::from_secs(60),
            Height::new(0, 1234).unwrap(),
            ProofSpecs::default(),
            vec![],
            AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
            None,
        )
        .unwrap()
        .into();
        client.client_state.insert((), client_state).unwrap();
        client.consensus_states.insert(10.into(), ()).unwrap();
        client.consensus_states.insert(20.into(), ()).unwrap();
        let client_id = IbcClientId::new(ClientType::new("07-tendermint".to_string()), 123)
            .unwrap()
            .into();
        ibc.clients.insert(client_id, client).unwrap();

        let conn_id = IbcConnectionId::new(123).into();
        ibc.connections.insert(conn_id, ()).unwrap();

        let channel_end_path = ChannelEndPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.channel_ends.insert(channel_end_path, ()).unwrap();

        let seq_sends_path = SeqSendPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.next_sequence_send.insert(seq_sends_path, 1).unwrap();

        let seq_recvs_path = SeqRecvPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.next_sequence_recv.insert(seq_recvs_path, 2).unwrap();

        let seq_acks_path = SeqAckPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.next_sequence_ack.insert(seq_acks_path, 3).unwrap();

        let commitments_path = CommitmentPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.commitments
            .insert(commitments_path, vec![1, 2, 3])
            .unwrap();

        let acks_path = AckPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.acks.insert(acks_path, vec![1, 2, 3]).unwrap();

        let receipts_path = ReceiptPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.receipts.insert(receipts_path, vec![1, 2, 3]).unwrap();

        let mut bytes = vec![];
        app.flush(&mut bytes).unwrap();
        assert_eq!(bytes, vec![0, 0]);

        let mut entries = store.range(..);
        let mut assert_next = |key: &[u8], value: &[u8]| {
            let (k, v) = entries.next().unwrap().unwrap();
            assert_eq!(
                String::from_utf8(k).unwrap(),
                String::from_utf8(key.to_vec()).unwrap()
            );
            assert_eq!(v, value);
        };

        assert_next(
            b"acks/ports/transfer/channels/channel-123/sequences/1",
            &[1, 2, 3],
        );
        assert_next(b"channelEnds/ports/transfer/channels/channel-123", &[]);
        assert_next(b"clients/07-tendermint-123/", &[0]);
        assert_next(
            b"clients/07-tendermint-123/clientState",
            &[
                0, 10, 43, 47, 105, 98, 99, 46, 108, 105, 103, 104, 116, 99, 108, 105, 101, 110,
                116, 115, 46, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 46, 118, 49, 46,
                67, 108, 105, 101, 110, 116, 83, 116, 97, 116, 101, 18, 90, 10, 5, 102, 111, 111,
                45, 48, 18, 4, 8, 1, 16, 3, 26, 4, 8, 128, 245, 36, 34, 4, 8, 128, 234, 73, 42, 2,
                8, 60, 50, 0, 58, 3, 16, 210, 9, 66, 25, 10, 9, 8, 1, 24, 1, 32, 1, 42, 1, 0, 18,
                12, 10, 2, 0, 1, 16, 33, 24, 4, 32, 12, 48, 1, 66, 25, 10, 9, 8, 1, 24, 1, 32, 1,
                42, 1, 0, 18, 12, 10, 2, 0, 1, 16, 32, 24, 1, 32, 1, 48, 1,
            ],
        );
        assert_next(b"clients/07-tendermint-123/consensusStates/10", &[]);
        assert_next(b"clients/07-tendermint-123/consensusStates/20", &[]);
        assert_next(
            b"commitments/ports/transfer/channels/channel-123/sequences/1",
            &[1, 2, 3],
        );
        assert_next(b"connections/connection-123", &[]);
        assert_next(
            b"nextSequenceAck/ports/transfer/channels/channel-123",
            &[0, 0, 0, 0, 0, 0, 0, 3],
        );
        assert_next(
            b"nextSequenceRecv/ports/transfer/channels/channel-123",
            &[0, 0, 0, 0, 0, 0, 0, 2],
        );
        assert_next(
            b"nextSequenceSend/ports/transfer/channels/channel-123",
            &[0, 0, 0, 0, 0, 0, 0, 1],
        );
        assert_next(
            b"receipts/ports/transfer/channels/channel-123/sequences/1",
            &[1, 2, 3],
        );
        assert!(entries.next().is_none());
    }
}
