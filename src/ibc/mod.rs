use crate::collections::Map;
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
    #[state(prefix = "clients/")]
    clients: Map<SlashTerminatedId, Client>,

    #[state(prefix = "connections/")]
    connections: Map<EofTerminatedId, Connection>,

    #[state(prefix = "channelEnds/")]
    channel_ends: Map<PortChannel, ChannelEnd>,

    #[state(prefix = "nextSequenceSend/")]
    next_sequence_send: Map<PortChannel, u64>,

    #[state(prefix = "nextSequenceRecv/")]
    next_sequence_recv: Map<PortChannel, u64>,

    #[state(prefix = "nextSequenceAck/")]
    next_sequence_ack: Map<PortChannel, u64>,

    #[state(prefix = "commitments/")]
    commitments: Map<PortChannelSequence, Vec<u8>>,

    #[state(prefix = "receipts/")]
    receipts: Map<PortChannelSequence, Vec<u8>>,

    #[state(prefix = "acks/")]
    acks: Map<PortChannelSequence, Vec<u8>>,
}

#[orga]
pub struct Client {
    #[state(prefix = "clientState")]
    client_state: ClientState,

    #[state(prefix = "consensusStates/")]
    consensus_states: Map<Number, ConsensusState>,
}

pub type SlashTerminatedId = ();
pub type EofTerminatedId = ();
pub type PortChannel = ();
pub type PortChannelSequence = ();
pub type Number = ();
pub type ClientState = ();
pub type ConsensusState = ();
pub type Connection = ();
pub type ChannelEnd = ();
