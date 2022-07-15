use super::{Adapter, Ibc, Lunchbox, ProtobufAdapter};
use crate::call::Call;
use crate::client::Client;
use crate::collections::{Deque, Map};
use crate::encoding::Encode;
use crate::query::Query;
use crate::state::State;
use crate::store::Write;
use ibc::{
    core::{
        ics02_client::{
            client_consensus::AnyConsensusState, client_state::AnyClientState,
            context::ClientReader,
        },
        ics03_connection::{
            connection::ConnectionEnd,
            context::{ConnectionKeeper, ConnectionReader},
            error::Error,
        },
        ics23_commitment::commitment::CommitmentPrefix,
        ics24_host::{
            identifier::{ClientId, ConnectionId},
            path::ConnectionsPath,
            Path,
        },
    },
    Height,
};

type Result<T> = std::result::Result<T, Error>;

impl From<crate::Error> for Error {
    fn from(_err: crate::Error) -> Error {
        Error::implementation_specific()
    }
}

impl Lunchbox {
    fn insert_connection<T: Into<ProtobufAdapter<ConnectionEnd>>>(
        &mut self,
        connection_id: ConnectionId,
        connection_end: T,
    ) -> crate::Result<()> {
        let key = Path::Connections(ConnectionsPath(connection_id)).into_bytes();
        self.0.put(key, connection_end.into().encode()?)
    }
}

#[derive(State, Call, Query, Client)]
pub struct ConnectionStore {
    count: u64,
    ends: Map<Adapter<ConnectionId>, ProtobufAdapter<ConnectionEnd>>,
    by_client: Map<Adapter<ClientId>, Deque<Adapter<ConnectionId>>>,
}

impl ConnectionReader for Ibc {
    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd> {
        self.connections
            .ends
            .get(conn_id.clone().into())?
            .map(|v| v.clone())
            .ok_or_else(|| Error::connection_not_found(conn_id.clone()))
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState> {
        println!("connections::client_state {:?}", client_id);
        ClientReader::client_state(self, client_id).map_err(Error::ics02_client)
    }

    fn host_current_height(&self) -> Height {
        Height::new(0, self.height).unwrap()
    }

    fn host_oldest_height(&self) -> Height {
        self.host_current_height()
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        todo!()
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState> {
        ClientReader::consensus_state(self, client_id, height).map_err(Error::ics02_client)
    }

    fn host_consensus_state(&self, height: Height) -> Result<AnyConsensusState> {
        ClientReader::host_consensus_state(self, height).map_err(Error::ics02_client)
    }

    fn connection_counter(&self) -> Result<u64> {
        Ok(self.connections.count)
    }
}

impl ConnectionKeeper for Ibc {
    fn store_connection(
        &mut self,
        connection_id: ConnectionId,
        connection_end: &ConnectionEnd,
    ) -> Result<()> {
        self.connections
            .ends
            .insert(connection_id.into(), connection_end.clone().into())?;

        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        connection_id: ConnectionId,
        client_id: &ClientId,
    ) -> Result<()> {
        self.connections
            .by_client
            .entry(client_id.clone().into())?
            .or_insert_default()?
            .push_back(connection_id.into())?;

        Ok(())
    }

    fn increase_connection_counter(&mut self) {
        self.connections.count += 1;
    }
}

// Calls and queries
impl ConnectionStore {
    #[query]
    pub fn get_by_conn_id(
        &self,
        id: Adapter<ConnectionId>,
    ) -> Result<ProtobufAdapter<ConnectionEnd>> {
        Ok(self
            .ends
            .get(id.clone())?
            .map(|v| v.clone())
            .ok_or_else(|| Error::connection_not_found(id.clone().into_inner()))?
            .into())
    }
}
