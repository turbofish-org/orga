use super::{
    ClientState, ClientStateV0, ClientV0, ClientV1, ConsensusState, ConsensusStateV0, IbcV0, IbcV1,
    IbcV2, Timestamp,
};
use crate::{
    collections::Map,
    migrate::{MigrateFrom, MigrateInto},
    state::State,
};

/// Upgrade from ibc-rs v0.15 -> v0.40.
/// This migration resets all IBC state.
impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(mut value: IbcV0) -> crate::Result<Self> {
        // TODO: implement actual IBC migration

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

/// Upgrade from ibc-rs v0.40 -> v0.42.
impl MigrateFrom<IbcV1> for IbcV2 {
    fn migrate_from(other: IbcV1) -> crate::Result<Self> {
        Ok(Self {
            height: other.height,
            host_consensus_states: other.host_consensus_states.migrate_into()?,
            channel_counter: other.channel_counter,
            connection_counter: other.connection_counter,
            client_counter: other.client_counter,
            transfer: other.transfer.migrate_into()?,
            clients: other.clients.migrate_into()?,
            connections: other.connections,
            channel_ends: other.channel_ends,
            next_sequence_send: other.next_sequence_send,
            next_sequence_recv: other.next_sequence_recv,
            next_sequence_ack: other.next_sequence_ack,
            commitments: other.commitments,
            receipts: other.receipts,
            acks: other.acks,
            store: other.store,
        })
    }
}

impl MigrateFrom<ClientV0> for ClientV1 {
    fn migrate_from(other: ClientV0) -> crate::Result<Self> {
        let other_client_state = other.client_state.get(())?;
        let mut client_state: Map<(), ClientState> = Default::default();
        if let Some(other_client_state) = other_client_state {
            client_state.insert((), other_client_state.clone().migrate_into()?)?;
        }

        Ok(Self {
            updates: other.updates.migrate_into()?,
            client_state, // TODO: use map migration (currently doesn't support empty keys)
            consensus_states: other.consensus_states.migrate_into()?,
            connections: other.connections.migrate_into()?,
            client_type: other.client_type,
        })
    }
}

impl MigrateFrom<ClientStateV0> for ClientState {
    fn migrate_from(other: ClientStateV0) -> crate::Result<Self> {
        Ok(Self { inner: other.inner })
    }
}

impl MigrateFrom<ConsensusStateV0> for ConsensusState {
    fn migrate_from(other: ConsensusStateV0) -> crate::Result<Self> {
        Ok(Self { inner: other.inner })
    }
}

impl MigrateFrom<i128> for Timestamp {
    fn migrate_from(other: i128) -> crate::Result<Self> {
        let nanos = other
            .try_into()
            .map_err(|_| crate::Error::Ibc("Invalid timestamp".to_string()))?;

        Ok(Self {
            inner: ibc::core::timestamp::Timestamp::from_nanoseconds(nanos)
                .map_err(|e| crate::Error::Ibc(e.to_string()))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::store::Write;
    use crate::{state::State, store::Store, Result};
    use ibc::clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    };

    use crate::ibc::Client;
    use ibc::{
        clients::ics07_tendermint::{client_state::AllowUpdate, trust_threshold::TrustThreshold},
        core::{
            ics02_client::{client_type::ClientType, height::Height},
            ics23_commitment::{commitment::CommitmentRoot, specs::ProofSpecs},
            ics24_host::identifier::ChainId,
        },
    };
    use tendermint::{Hash, Time};

    #[test]
    fn migrate_consensus_state() -> Result<()> {
        let bytes_v0 = vec![
            10, 46, 47, 105, 98, 99, 46, 108, 105, 103, 104, 116, 99, 108, 105, 101, 110, 116, 115,
            46, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 46, 118, 49, 46, 67, 111, 110,
            115, 101, 110, 115, 117, 115, 83, 116, 97, 116, 101, 18, 72, 10, 0, 18, 34, 10, 32, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 26, 32, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5,
        ];
        let bytes_v1 = vec![
            10, 0, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 32, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
        ];
        let store = Store::with_map_store();
        let consensus_state = ConsensusStateV0::load(store, &mut bytes_v0.as_slice())?;
        let consensus_state: ConsensusState = consensus_state.migrate_into()?;

        let mut out_bytes = vec![];
        consensus_state.flush(&mut out_bytes)?;
        assert_eq!(out_bytes, bytes_v1);

        Ok(())
    }

    #[test]
    fn migrate_client_state() -> Result<()> {
        let bytes_v0 = vec![
            10, 43, 47, 105, 98, 99, 46, 108, 105, 103, 104, 116, 99, 108, 105, 101, 110, 116, 115,
            46, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116, 46, 118, 49, 46, 67, 108, 105,
            101, 110, 116, 83, 116, 97, 116, 101, 18, 90, 10, 5, 102, 111, 111, 45, 48, 18, 4, 8,
            1, 16, 3, 26, 4, 8, 128, 245, 36, 34, 4, 8, 128, 234, 73, 42, 2, 8, 60, 50, 0, 58, 3,
            16, 210, 9, 66, 25, 10, 9, 8, 1, 24, 1, 32, 1, 42, 1, 0, 18, 12, 10, 2, 0, 1, 16, 33,
            24, 4, 32, 12, 48, 1, 66, 25, 10, 9, 8, 1, 24, 1, 32, 1, 42, 1, 0, 18, 12, 10, 2, 0, 1,
            16, 32, 24, 1, 32, 1, 48, 1,
        ];
        let bytes_v1 = vec![
            10, 5, 102, 111, 111, 45, 48, 18, 4, 8, 1, 16, 3, 26, 4, 8, 128, 245, 36, 34, 4, 8,
            128, 234, 73, 42, 2, 8, 60, 50, 0, 58, 3, 16, 210, 9, 66, 25, 10, 9, 8, 1, 24, 1, 32,
            1, 42, 1, 0, 18, 12, 10, 2, 0, 1, 16, 33, 24, 4, 32, 12, 48, 1, 66, 25, 10, 9, 8, 1,
            24, 1, 32, 1, 42, 1, 0, 18, 12, 10, 2, 0, 1, 16, 32, 24, 1, 32, 1, 48, 1,
        ];

        let store = Store::with_map_store();
        let client_state = ClientStateV0::load(store, &mut bytes_v0.as_slice())?;
        let client_state: ClientState = client_state.migrate_into()?;

        let mut out_bytes = vec![];
        client_state.flush(&mut out_bytes)?;
        assert_eq!(out_bytes, bytes_v1);

        Ok(())
    }

    #[test]
    fn client_migration() -> Result<()> {
        let raw_client_state = TmClientState::new(
            ChainId::new("foo", 0),
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
        )
        .unwrap();
        let mut client = ClientV0::default();
        let mut store = Store::with_map_store();
        client.attach(store.clone())?;
        client
            .client_state
            .insert((), raw_client_state.clone().into())?;

        let raw_consensus_state = TmConsensusState::new(
            CommitmentRoot::from_bytes(&[0; 32]),
            Time::from_unix_timestamp(0, 1_000_000).unwrap(),
            Hash::Sha256([5; 32]),
        );
        client.consensus_states.insert(
            "0-100".to_string().into(),
            raw_consensus_state.clone().into(),
        )?;

        client.client_type = ClientType::new("07-tendermint").unwrap().into();
        client
            .updates
            .insert(
                "0-100".to_string().into(),
                (1_000_000, Height::new(0, 100).unwrap().into()),
            )
            .unwrap();

        let mut out_bytes = vec![];
        client.flush(&mut out_bytes)?;
        assert_eq!(
            out_bytes,
            [0, 48, 55, 45, 116, 101, 110, 100, 101, 114, 109, 105, 110, 116,]
        );
        store.put(vec![], out_bytes.clone())?;

        let client = Client::load(store.clone(), &mut out_bytes.as_slice())?;

        let client_state = client.client_state.get(())?.unwrap();
        assert_eq!(client_state.inner, raw_client_state);

        let consensus_state = client
            .consensus_states
            .get("0-100".to_string().into())?
            .unwrap();
        assert_eq!(consensus_state.inner, raw_consensus_state);

        Ok(())
    }
}
