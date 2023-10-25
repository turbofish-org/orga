use super::{IbcContextV0, IbcContextV1, IbcV0, IbcV1, IbcV2, IbcV3, PortChannelSequence};
use crate::collections::Map;
use crate::migrate::MigrateFrom;
use crate::state::State;

impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(_value: IbcV0) -> crate::Result<Self> {
        unreachable!()
    }
}

impl MigrateFrom<IbcV1> for IbcV2 {
    fn migrate_from(_value: IbcV1) -> crate::Result<Self> {
        unreachable!()
    }
}

impl MigrateFrom<IbcV2> for IbcV3 {
    fn migrate_from(mut value: IbcV2) -> crate::Result<Self> {
        value
            .root_store
            .remove_range(b"a".to_vec()..b"z".to_vec())?;

        value.local_store.remove_range(..)?;

        let mut out = vec![];
        value.root_store.flush(&mut out)?;
        value.local_store.flush(&mut out)?;

        Ok(Self::default())
    }
}

impl MigrateFrom<IbcContextV0> for IbcContextV1 {
    fn migrate_from(value: IbcContextV0) -> crate::Result<Self> {
        let mut receipts: Map<PortChannelSequence, u8> = Map::default();
        for entry in value.receipts.iter()? {
            let (k, _v) = entry?;
            receipts.insert(k.clone(), 1)?;
        }

        Ok(Self {
            height: value.height,
            host_consensus_states: value.host_consensus_states,
            channel_counter: value.channel_counter,
            connection_counter: value.connection_counter,
            client_counter: value.client_counter,
            clients: value.clients,
            connections: value.connections,
            channel_ends: value.channel_ends,
            next_sequence_send: value.next_sequence_send,
            next_sequence_recv: value.next_sequence_recv,
            next_sequence_ack: value.next_sequence_ack,
            commitments: value.commitments,
            receipts,
            acks: value.acks,
            store: value.store,
        })
    }
}
