use ibc::core::ics04_channel::packet::Sequence;
use ibc::core::ics24_host::identifier::{
    ChannelId, ClientId as IbcClientId, ConnectionId as IbcConnectionId, PortId,
};
use ibc::core::ics24_host::path::{
    AcksPath, ChannelEndsPath, CommitmentsPath, ReceiptsPath, SeqAcksPath, SeqRecvsPath,
    SeqSendsPath,
};
use serde::Serialize;

use crate::collections::Map;
use crate::encoding::{ByteTerminatedString, Decode, Encode, EofTerminatedString, FixedString};
use crate::orga;

// "clients/{identifier}/clientState"	ClientState	ICS 2
// "clients/{identifier}/consensusStates/{height}"	ConsensusState	ICS 7
// "connections/{identifier}"	ConnectionEnd	ICS 3
// "channelEnds/ports/{identifier}/channels/{identifier}"	ChannelEnd	ICS 4
// "nextSequenceSend/ports/{identifier}/channels/{identifier}"	uint64	ICS 4
// "nextSequenceRecv/ports/{identifier}/channels/{identifier}"	uint64	ICS 4
// "nextSequenceAck/ports/{identifier}/channels/{identifier}"	uint64	ICS 4
// "commitments/ports/{identifier}/channels/{identifier}/sequences/{sequence}"	bytes	ICS 4
// "receipts/ports/{identifier}/channels/{identifier}/sequences/{sequence}"	bytes	ICS 4
// "acks/ports/{identifier}/channels/{identifier}/sequences/{sequence}"	bytes	ICS 4

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

port_channel_from_impl!(ChannelEndsPath);
port_channel_from_impl!(SeqSendsPath);
port_channel_from_impl!(SeqRecvsPath);
port_channel_from_impl!(SeqAcksPath);

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

port_channel_sequence_from_impl!(CommitmentsPath);
port_channel_sequence_from_impl!(AcksPath);
port_channel_sequence_from_impl!(ReceiptsPath);

// TODO
pub type ClientState = ();
pub type ConsensusState = ();
pub type Connection = ();
pub type ChannelEnd = ();

#[cfg(test)]
mod tests {
    use ibc::core::ics02_client::client_type::ClientType;

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
        client.client_state.insert((), ()).unwrap();
        client.consensus_states.insert(10.into(), ()).unwrap();
        client.consensus_states.insert(20.into(), ()).unwrap();
        let client_id = IbcClientId::new(ClientType::Tendermint, 123)
            .unwrap()
            .into();
        ibc.clients.insert(client_id, client).unwrap();

        let conn_id = IbcConnectionId::new(123).into();
        ibc.connections.insert(conn_id, ()).unwrap();

        let channel_end_path = ChannelEndsPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.channel_ends.insert(channel_end_path, ()).unwrap();

        let seq_sends_path = SeqSendsPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.next_sequence_send.insert(seq_sends_path, 1).unwrap();

        let seq_recvs_path = SeqRecvsPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.next_sequence_recv.insert(seq_recvs_path, 2).unwrap();

        let seq_acks_path = SeqAcksPath(PortId::transfer(), ChannelId::new(123)).into();
        ibc.next_sequence_ack.insert(seq_acks_path, 3).unwrap();

        let commitments_path = CommitmentsPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.commitments
            .insert(commitments_path, vec![1, 2, 3])
            .unwrap();

        let acks_path = AcksPath {
            port_id: PortId::transfer(),
            channel_id: ChannelId::new(123),
            sequence: 1.into(),
        }
        .into();
        ibc.acks.insert(acks_path, vec![1, 2, 3]).unwrap();

        let receipts_path = ReceiptsPath {
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
        let mut assert_next_key = |key: &[u8]| {
            assert_eq!(
                String::from_utf8(entries.next().unwrap().unwrap().0).unwrap(),
                String::from_utf8(key.to_vec()).unwrap()
            );
        };

        assert_next_key(b"acks/ports/transfer/channels/channel-123/sequences/1");
        assert_next_key(b"channelEnds/ports/transfer/channels/channel-123");
        assert_next_key(b"clients/07-tendermint-123/");
        assert_next_key(b"clients/07-tendermint-123/clientState");
        assert_next_key(b"clients/07-tendermint-123/consensusStates/10");
        assert_next_key(b"clients/07-tendermint-123/consensusStates/20");
        assert_next_key(b"commitments/ports/transfer/channels/channel-123/sequences/1");
        assert_next_key(b"connections/connection-123");
        assert_next_key(b"nextSequenceAck/ports/transfer/channels/channel-123");
        assert_next_key(b"nextSequenceRecv/ports/transfer/channels/channel-123");
        assert_next_key(b"nextSequenceSend/ports/transfer/channels/channel-123");
        assert_next_key(b"receipts/ports/transfer/channels/channel-123/sequences/1");
        assert!(entries.next().is_none());
    }
}
