use super::Ibc;
use ibc::{
    core::{
        ics02_client::{client_consensus::AnyConsensusState, client_state::AnyClientState},
        ics03_connection::{
            connection::ConnectionEnd,
            context::{ConnectionKeeper, ConnectionReader},
        },
        ics23_commitment::commitment::CommitmentPrefix,
        ics24_host::identifier::{ClientId, ConnectionId},
    },
    Height,
};

type Result<T> = std::result::Result<T, ibc::core::ics03_connection::error::Error>;

impl ConnectionReader for Ibc {
    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd> {
        todo!()
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState> {
        todo!()
    }

    fn host_current_height(&self) -> Height {
        todo!()
    }

    fn host_oldest_height(&self) -> Height {
        todo!()
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        todo!()
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState> {
        todo!()
    }

    fn host_consensus_state(&self, height: Height) -> Result<AnyConsensusState> {
        todo!()
    }

    fn connection_counter(&self) -> Result<u64> {
        todo!()
    }
}

impl ConnectionKeeper for Ibc {
    fn store_connection(
        &mut self,
        connection_id: ConnectionId,
        connection_end: &ConnectionEnd,
    ) -> Result<()> {
        todo!()
    }

    fn store_connection_to_client(
        &mut self,
        connection_id: ConnectionId,
        client_id: &ClientId,
    ) -> Result<()> {
        todo!()
    }

    fn increase_connection_counter(&mut self) {
        todo!()
    }
}
