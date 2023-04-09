use ibc::core::ics24_host::identifier::ClientId as IbcClientId;

use crate::collections::Map;
use crate::encoding::ByteTerminatedString;
use crate::orga;

// provableStore	"clients/{identifier}/clientState"	ClientState	ICS 2
// provableStore	"clients/{identifier}/consensusStates/{height}"	ConsensusState	ICS 7
// provableStore	"connections/{identifier}"	ConnectionEnd	ICS 3
// provableStore	"channelEnds/ports/{identifier}/channels/{identifier}"	ChannelEnd	ICS 4
// provableStore	"nextSequenceSend/ports/{identifier}/channels/{identifier}"	uint64	ICS 4
// provableStore	"nextSequenceRecv/ports/{identifier}/channels/{identifier}"	uint64	ICS 4
// provableStore	"nextSequenceAck/ports/{identifier}/channels/{identifier}"	uint64	ICS 4
// provableStore	"commitments/ports/{identifier}/channels/{identifier}/sequences/{sequence}"	bytes	ICS 4
// provableStore	"receipts/ports/{identifier}/channels/{identifier}/sequences/{sequence}"	bytes	ICS 4
// provableStore	"acks/ports/{identifier}/channels/{identifier}/sequences/{sequence}"	bytes	ICS 4

#[orga]
pub struct Ibc {
    #[state(absolute_prefix(b"clients/"))]
    clients: Map<ClientId, Client>,

    #[state(absolute_prefix(b"connections/"))]
    connections: Map<EofTerminatedId, Connection>,

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
    #[state(prefix(b"clientState/"))]
    client_state: ClientState,

    #[state(prefix(b"consensusStates/"))]
    consensus_states: Map<Number, ConsensusState>,
}

pub type ClientId = ByteTerminatedString<IbcClientId, b'/'>;

// TODO
pub type EofTerminatedId = ();
pub type PortChannel = ();
pub type PortChannelSequence = ();
pub type Number = ();
pub type ClientState = ();
pub type ConsensusState = ();
pub type Connection = ();
pub type ChannelEnd = ();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{Decode, Encode};
    use crate::state::State;
    use crate::store::Store;

    #[test]
    fn client_id_encode_decode() {
        let id: IbcClientId = "07-tendermint-0".parse().unwrap();
        let id: ClientId = ByteTerminatedString(id);

        let mut bytes = id.encode().unwrap();
        assert_eq!(bytes, b"07-tendermint-0/");

        bytes.extend_from_slice(b"foo/bar/123");
        let decoded = ClientId::decode(&bytes[..]).unwrap();
        assert_eq!(*decoded, *id);
    }

    #[test]
    fn client_id_state() {
        let id: IbcClientId = "07-tendermint-0".parse().unwrap();
        let id: ClientId = ByteTerminatedString(id);

        let mut bytes = vec![];
        id.clone().flush(&mut bytes).unwrap();
        assert_eq!(bytes, b"07-tendermint-0/");

        bytes.extend_from_slice(b"foo/bar/123");
        let decoded = ClientId::load(Store::default(), &mut &bytes[..]).unwrap();
        assert_eq!(*decoded, *id);
    }
}
