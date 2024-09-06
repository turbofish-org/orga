use ibc::core::client::context::client_state::ClientStateValidation;
use ibc::core::client::types::Height;
use ibc::core::client::types::Status;
use ibc::core::host::types::path::Path;
use ibc_proto::ibc::core::channel::v1::{Channel, IdentifiedChannel, PacketState};
use ibc_proto::ibc::core::client::v1::{ConsensusStateWithHeight, IdentifiedClientState};
use ibc_proto::ibc::core::connection::v1::{
    ConnectionEnd as RawConnectionEnd, IdentifiedConnection,
};
use ics23::LeafOp;
use std::str::FromStr;
use tendermint_proto::v0_34::abci::{RequestQuery, ResponseQuery};
use tendermint_proto::v0_34::crypto::{ProofOp, ProofOps};

use super::{
    ClientIdKey, ConnectionEnd, ConnectionIdKey, Ibc, IbcContext, PortChannel, IBC_QUERY_PATH,
};
use crate::abci::AbciQuery;
use crate::encoding::LengthVec;
use crate::store::Read;
use crate::{Error, Result};
use ibc::primitives::prelude::*;

impl AbciQuery for Ibc {
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        self.ctx.abci_query(req)
    }
}

#[cfg(feature = "abci")]
impl AbciQuery for IbcContext {
    /// This implementation exists to support raw abci queries for IBC data, as
    /// required by relayers like Hermes, with proofs verifiable by other
    /// IBC chains.
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        if req.path != IBC_QUERY_PATH {
            return Err(Error::Ibc("Invalid query path".to_string()));
        }
        let data = req.data.to_vec();

        let path: Path = String::from_utf8(data.clone())
            .map_err(|_| Error::Ibc("Invalid query data encoding".to_string()))?
            .parse()
            .map_err(|_| Error::Ibc("Invalid query data".to_string()))?;

        let value_bytes = self.store.get(&data)?.unwrap_or_default();
        let key = path.clone().into_bytes();

        use prost::Message;

        let mut outer_proof_bytes = vec![];
        let inner_root_hash = self.store.backing_store().borrow().root_hash();

        let outer_proof = ics23::CommitmentProof {
            proof: Some(ics23::commitment_proof::Proof::Exist(
                ics23::ExistenceProof {
                    key: b"ibc".to_vec(),
                    value: inner_root_hash.to_vec(),
                    leaf: Some(LeafOp {
                        hash: 6,
                        length: 0,
                        prehash_key: 0,
                        prehash_value: 0,
                        prefix: vec![],
                    }),
                    path: vec![],
                },
            )),
        };
        outer_proof
            .encode(&mut outer_proof_bytes)
            .map_err(|_| Error::Ibc("Failed to create outer proof".into()))?;

        let mut proof_bytes = vec![];
        let proof = self
            .store
            .backing_store()
            .borrow()
            .create_ics23_proof(key.as_slice())?;

        proof
            .encode(&mut proof_bytes)
            .map_err(|_| Error::Ibc("Failed to create proof".into()))?;

        Ok(ResponseQuery {
            code: 0,
            key: req.data.clone(),
            value: value_bytes.into(),
            proof_ops: Some(ProofOps {
                ops: vec![
                    ProofOp {
                        r#type: "".to_string(),
                        key: path.into_bytes(),
                        data: proof_bytes,
                    },
                    ProofOp {
                        r#type: "".to_string(),
                        key: b"ibc".to_vec(),
                        data: outer_proof_bytes,
                    },
                ],
            }),
            height: self.height as i64,
            ..Default::default()
        })
    }
}

/// The methods in this impl block are used to support the gRPC services in
/// [super::service].
impl IbcContext {
    /// Returns the current height of the chain.
    pub fn query_height(&self) -> Result<u64> {
        Ok(self.height)
    }

    /// Returns all client states with their client identifiers.
    pub fn query_client_states(&self) -> Result<Vec<IdentifiedClientState>> {
        let mut states = vec![];
        for entry in self.clients.iter()? {
            let (id, client) = entry?;
            for entry in client.client_state.iter()? {
                let (_, client_state) = entry?;
                states.push(IdentifiedClientState {
                    client_id: id.clone().as_str().to_string(),
                    client_state: Some(client_state.clone().inner.into()),
                });
            }
        }

        Ok(states)
    }

    /// Returns the consensus state for a client at the provided height.
    pub fn query_consensus_state(
        &self,
        client_id: ClientIdKey,
        revision_number: u64,
        revision_height: u64,
    ) -> Result<ConsensusStateWithHeight> {
        let client = self
            .clients
            .get(client_id)?
            .ok_or_else(|| Error::Ibc("Client not found".to_string()))?;
        let height = Height::new(revision_number, revision_height)
            .map_err(|_| Error::Ibc("Invalid height".to_string()))?;
        let consensus_state = client
            .consensus_states
            .get(height.into())?
            .ok_or_else(|| Error::Ibc("Consensus state not found".to_string()))?;

        Ok(ConsensusStateWithHeight {
            height: Some(height.into()),
            consensus_state: Some(consensus_state.clone().inner.into()),
        })
    }

    /// Returns all consensus states for a client.
    pub fn query_consensus_states(
        &self,
        client_id: ClientIdKey,
    ) -> Result<Vec<ConsensusStateWithHeight>> {
        let mut states = vec![];

        let client = self
            .clients
            .get(client_id)?
            .ok_or_else(|| Error::Ibc("Client not found".to_string()))?;

        for entry in client.consensus_states.iter()? {
            let (height, consensus_state) = entry?;
            let height: Height = height.clone().try_into()?;
            states.push(ConsensusStateWithHeight {
                height: Some(height.into()),
                consensus_state: Some(consensus_state.clone().inner.into()),
            });
        }

        Ok(states)
    }

    /// Returns the connection with the provided connection identifier if it
    /// exists.
    pub fn query_connection(&self, conn_id: ConnectionIdKey) -> Result<Option<ConnectionEnd>> {
        Ok(self
            .connections
            .get(conn_id)?
            .map(|connection_end| connection_end.clone())
            .map(|connection_end| connection_end.into()))
    }

    /// Returns all connections with their connection identifiers.
    pub fn query_all_connections(&self) -> Result<Vec<IdentifiedConnection>> {
        let mut connections = vec![];

        for entry in self.connections.iter()? {
            let (id, connection) = entry?;
            let connection: ConnectionEnd = connection.clone().into();
            let raw_connection: RawConnectionEnd = connection.into();
            connections.push(IdentifiedConnection {
                client_id: raw_connection.client_id,
                counterparty: raw_connection.counterparty,
                versions: raw_connection.versions,
                delay_period: raw_connection.delay_period,
                id: id.clone().as_str().to_string(),
                state: raw_connection.state,
            });
        }

        Ok(connections)
    }

    /// Returns all connection identifiers associated with a client.
    pub fn query_client_connections(&self, client_id: ClientIdKey) -> Result<Vec<ConnectionIdKey>> {
        let mut connection_ids = vec![];

        let client = self
            .clients
            .get(client_id)?
            .ok_or_else(|| Error::Ibc("Client not found".to_string()))?;

        for entry in client.connections.iter()? {
            let (id, _) = entry?;
            connection_ids.push(id.clone());
        }

        Ok(connection_ids)
    }

    /// Returns the channel with the provided port and channel identifiers if it
    /// exists.
    pub fn query_channel(&self, port_chan: PortChannel) -> Result<Option<Channel>> {
        let channel = self
            .channel_ends
            .get(port_chan)?
            .map(|channel_end| channel_end.clone().into());

        Ok(channel)
    }

    /// Returns all channels with their port and channel identifiers.
    pub fn query_all_channels(&self) -> Result<Vec<IdentifiedChannel>> {
        let mut channels = vec![];
        for entry in self.channel_ends.iter()? {
            let (path, channel_end) = entry?;
            let channel_end: Channel = channel_end.clone().into();
            channels.push(IdentifiedChannel {
                port_id: path.clone().1.to_string(),
                channel_id: path.clone().3.to_string(),
                version: channel_end.version,
                connection_hops: channel_end.connection_hops,
                counterparty: channel_end.counterparty,
                ordering: channel_end.ordering,
                state: channel_end.state,
                upgrade_sequence: 0,
            });
        }

        Ok(channels)
    }

    /// Returns all channels associated with a connection.
    pub fn query_connection_channels(
        &self,
        conn_id: ConnectionIdKey,
    ) -> Result<Vec<IdentifiedChannel>> {
        let channels = self
            .query_all_channels()?
            .into_iter()
            .filter(|channel| {
                channel.connection_hops.first() == Some(&conn_id.clone().as_str().to_string())
            })
            .collect();

        Ok(channels)
    }

    /// Returns all packet commitments for a channel.
    pub fn query_packet_commitments(&self, port_chan: PortChannel) -> Result<Vec<PacketState>> {
        let mut commitments = vec![];

        // TODO: instead of filtering, use self.commitments.range()
        for entry in self.commitments.iter()? {
            let (path, data) = entry?;
            if path.port_id()? != port_chan.port_id()?
                || path.channel_id()? != port_chan.channel_id()?
                || data.is_empty()
            {
                continue;
            }
            commitments.push(PacketState {
                port_id: path.port_id()?.to_string(),
                channel_id: path.channel_id()?.to_string(),
                sequence: path.sequence()?.to_string().parse()?,
                data: data.clone(),
            });
        }

        Ok(commitments)
    }

    /// Returns all unreceived packets for a channel.
    pub fn query_unreceived_packets(
        &self,
        port_chan: PortChannel,
        sequences: LengthVec<u16, u64>,
    ) -> Result<Vec<u64>> {
        let mut unreceived = vec![];
        let sequences: Vec<_> = sequences.into();
        for sequence in sequences.into_iter() {
            let path = port_chan.clone().with_sequence(sequence.into())?;
            if !self.receipts.contains_key(path)? {
                unreceived.push(sequence);
            }
        }

        Ok(unreceived)
    }

    /// Returns all unreceived acks for a channel.
    pub fn query_unreceived_acks(
        &self,
        port_chan: PortChannel,
        sequences: LengthVec<u16, u64>,
    ) -> Result<Vec<u64>> {
        let mut unreceived = vec![];
        let sequences: Vec<_> = sequences.into();

        for sequence in sequences.into_iter() {
            let path = port_chan.clone().with_sequence(sequence.into())?;
            if self.commitments.contains_key(path)? {
                unreceived.push(sequence);
            }
        }

        Ok(unreceived)
    }

    /// Returns all packet acks for a channel.
    pub fn query_packet_acks(
        &self,
        sequences: LengthVec<u8, u64>,
        port_chan: PortChannel,
    ) -> Result<Vec<PacketState>> {
        let mut acks = vec![];
        for seq in sequences.iter() {
            let path = port_chan.clone().with_sequence((*seq).into())?;
            let entry = self.acks.get(path)?;
            if let Some(data) = entry {
                if data.is_empty() {
                    continue;
                }
                acks.push(PacketState {
                    port_id: port_chan.port_id()?.to_string(),
                    channel_id: port_chan.channel_id()?.to_string(),
                    sequence: *seq,
                    data: data.clone(),
                });
            }
        }

        Ok(acks)
    }

    /// Returns the next sequence receive for a channel.
    pub fn query_next_sequence_receive(&self, port_chan: PortChannel) -> crate::Result<u64> {
        let sequence_string = self
            .next_sequence_recv
            .get(port_chan)?
            .ok_or(Error::Ibc("Sequence not found".to_string()))?;
        let sequence = u64::from_str(&sequence_string.to_string())?;
        Ok(sequence)
    }

    /// Returns the next sequence send for a channel.
    pub fn query_next_sequence_send(&self, port_chan: PortChannel) -> crate::Result<u64> {
        let sequence_string = self
            .next_sequence_send
            .get(port_chan)?
            .ok_or(Error::Ibc("Sequence not found".to_string()))?;
        let sequence = u64::from_str(&sequence_string.to_string())?;
        Ok(sequence)
    }

    /// Returns the client status for the provided client identifier.
    pub fn query_client_status(&self, client_id: ClientIdKey) -> Result<Status> {
        let client = self
            .clients
            .get(client_id.clone())?
            .ok_or_else(|| Error::Ibc("Client not found".to_string()))?;
        let client_state = client
            .client_state
            .get(Default::default())?
            .ok_or_else(|| Error::Ibc("Client not found".to_string()))?;
        client_state
            .inner
            .status(self, &client_id.0)
            .map_err(|e| Error::Ibc(e.to_string()))
    }
}
