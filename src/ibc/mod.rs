use ibc::core::ics04_channel::packet::Sequence;
use ibc::core::ics24_host::identifier::{
    ChannelId, ClientId as IbcClientId, ConnectionId as IbcConnectionId, PortId,
};

use crate::collections::Map;
use crate::encoding::{ByteTerminatedString, EofTerminatedString};
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

pub type ClientId = ByteTerminatedString<b'/', IbcClientId>;
pub type ConnectionId = EofTerminatedString<IbcConnectionId>;
pub type PortChannel = (
    ByteTerminatedString<b'/', PortId>,
    EofTerminatedString<ChannelId>,
);
pub type PortChannelSequence = (
    ByteTerminatedString<b'/', PortId>,
    ByteTerminatedString<b'/', ChannelId>,
    EofTerminatedString<Sequence>,
);
pub type Height = EofTerminatedString<u64>;

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
        client.consensus_states.insert(1.into(), ()).unwrap();
        client.consensus_states.insert(2.into(), ()).unwrap();
        let client_id = IbcClientId::new(ClientType::Tendermint, 123)
            .unwrap()
            .into();
        ibc.clients.insert(client_id, client).unwrap();

        let mut client = Client::default();
        client.client_state.insert((), ()).unwrap();
        client.consensus_states.insert(10.into(), ()).unwrap();
        client.consensus_states.insert(20.into(), ()).unwrap();
        let client_id = IbcClientId::new(ClientType::Tendermint, 124)
            .unwrap()
            .into();
        ibc.clients.insert(client_id, client).unwrap();

        // TODO: fill in rest of state

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
        assert_next_key(b"clients/07-tendermint-123/");
        assert_next_key(b"clients/07-tendermint-123/clientState");
        assert_next_key(b"clients/07-tendermint-123/consensusStates/1");
        assert_next_key(b"clients/07-tendermint-123/consensusStates/2");
        assert_next_key(b"clients/07-tendermint-124/");
        assert_next_key(b"clients/07-tendermint-124/clientState");
        assert_next_key(b"clients/07-tendermint-124/consensusStates/10");
        assert_next_key(b"clients/07-tendermint-124/consensusStates/20");
        assert!(entries.next().is_none());
    }
}
