use std::time::Duration;
use std::{iter::DoubleEndedIterator, ops::Bound};

use ibc::core::ics23_commitment::commitment::CommitmentRoot;
use ibc::{
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        context::Router,
        ics02_client::{
            client_state::ClientState, client_type::ClientType, consensus_state::ConsensusState,
            error::ClientError,
        },
        ics03_connection::error::ConnectionError,
        ics04_channel::{
            commitment::{AcknowledgementCommitment, PacketCommitment},
            error::{ChannelError, PacketError},
            msgs::{ChannelMsg, PacketMsg},
            packet::Receipt,
        },
        ics23_commitment::commitment::CommitmentPrefix,
        ics24_host::path::{
            ClientConnectionPath, ClientConsensusStatePath, ClientStatePath, ConnectionPath,
        },
        ics26_routing::context::{Module, ModuleId},
        ContextError, ExecutionContext, ValidationContext,
    },
    events::IbcEvent,
    timestamp::Timestamp,
    Height,
};

use crate::abci::BeginBlock;
use crate::context::GetContext;
use crate::plugins::{BeginBlockCtx, Events, Time};
use crate::Error;

use super::*;

const MAX_HOST_CONSENSUS_STATES: u64 = 100_000;

#[cfg(feature = "abci")]
impl BeginBlock for Ibc {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> crate::Result<()> {
        self.height = ctx.height;

        let timestamp = if let Some(ref timestamp) = ctx.header.time {
            tendermint::Time::from_unix_timestamp(timestamp.seconds, timestamp.nanos as u32)
                .map_err(|_| crate::Error::Ibc("Invalid timestamp".to_string()))?
        } else {
            return Err(Error::Tendermint("Missing timestamp".to_string()));
        };

        self.host_consensus_states.push_back(
            TmConsensusState::new(
                CommitmentRoot::from_bytes(ctx.header.app_hash.as_slice()),
                timestamp,
                tendermint::Hash::Sha256(
                    ctx.header
                        .next_validators_hash
                        .clone()
                        .try_into()
                        .map_err(|_| Error::Tendermint("Invalid hash".to_string()))?,
                ),
            )
            .into(),
        )?;

        while self.host_consensus_states.len() > MAX_HOST_CONSENSUS_STATES {
            self.host_consensus_states.pop_front()?;
        }

        Ok(())
    }
}

impl Router for Ibc {
    fn get_route(&self, module_id: &ModuleId) -> Option<&dyn Module> {
        todo!()
    }

    fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn Module> {
        todo!()
    }

    fn has_route(&self, module_id: &ModuleId) -> bool {
        todo!()
    }

    fn lookup_module_by_port(&self, port_id: &PortId) -> Option<ModuleId> {
        todo!()
    }

    fn lookup_module_channel(&self, msg: &ChannelMsg) -> Result<ModuleId, ChannelError> {
        todo!()
    }

    fn lookup_module_packet(&self, msg: &PacketMsg) -> Result<ModuleId, ChannelError> {
        todo!()
    }
}

impl ValidationContext for Ibc {
    fn client_state(&self, client_id: &IbcClientId) -> Result<Box<dyn ClientState>, ContextError> {
        Ok(Box::<TmClientState>::new(
            self.clients
                .get(client_id.clone().into())
                .map_err(|_| ClientError::ImplementationSpecific)?
                .ok_or_else(|| ClientError::ClientStateNotFound {
                    client_id: client_id.clone(),
                })?
                .client_state
                .get(())
                .map_err(|_| ClientError::ImplementationSpecific)?
                .ok_or(ClientError::ImplementationSpecific)?
                .clone()
                .into(),
        ))
    }

    fn decode_client_state(&self, client_state: Any) -> Result<Box<dyn ClientState>, ContextError> {
        if let Ok(client_state) = TmClientState::try_from(client_state.clone()) {
            Ok(client_state.into_box())
        } else {
            Err(ClientError::UnknownClientStateType {
                client_state_type: client_state.type_url,
            })
            .map_err(ContextError::from)
        }
    }

    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Box<dyn ConsensusState>, ContextError> {
        Ok(Box::<TmConsensusState>::new(
            self.clients
                .get(client_cons_state_path.client_id.clone().into())
                .map_err(|_| ClientError::ImplementationSpecific)?
                .ok_or_else(|| ClientError::ClientStateNotFound {
                    client_id: client_cons_state_path.client_id.clone(),
                })?
                .consensus_states
                .get(
                    Height::new(client_cons_state_path.epoch, client_cons_state_path.height)
                        .map_err(|_| ClientError::ImplementationSpecific)?
                        .into(),
                )
                .map_err(|_| ClientError::ImplementationSpecific)?
                .ok_or(ClientError::ImplementationSpecific)?
                .clone()
                .into(),
        ))
    }

    fn next_consensus_state(
        &self,
        client_id: &IbcClientId,
        height: &Height,
    ) -> Result<Option<Box<dyn ConsensusState>>, ContextError> {
        let end_height = Height::new(height.revision_number() + 1, 1)
            .map_err(|_| ClientError::ImplementationSpecific)?;
        self.clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .ok_or_else(|| ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .consensus_states
            .range((
                Bound::<EofTerminatedString>::Excluded((*height).into()),
                Bound::Excluded(end_height.into()),
            ))
            .map_err(|_| ClientError::ImplementationSpecific)?
            .next()
            .map(|res| {
                res.map(|(_, v)| {
                    Box::<TmConsensusState>::new(v.clone().into()) as Box<dyn ConsensusState>
                })
            })
            .transpose()
            .map_err(|_| ContextError::ClientError(ClientError::ImplementationSpecific))
    }

    fn prev_consensus_state(
        &self,
        client_id: &IbcClientId,
        height: &Height,
    ) -> Result<Option<Box<dyn ConsensusState>>, ContextError> {
        let end_height = Height::new(height.revision_number(), 1)
            .map_err(|_| ClientError::ImplementationSpecific)?;
        self.clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .ok_or_else(|| ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .consensus_states
            .range((
                Bound::<EofTerminatedString>::Included(end_height.into()),
                Bound::Excluded((*height).into()),
            ))
            .map_err(|_| ClientError::ImplementationSpecific)?
            .next_back()
            .map(|res| {
                res.map(|(_, v)| {
                    Box::<TmConsensusState>::new(v.clone().into()) as Box<dyn ConsensusState>
                })
            })
            .transpose()
            .map_err(|_| ContextError::ClientError(ClientError::ImplementationSpecific))
    }

    fn host_height(&self) -> Result<Height, ContextError> {
        Ok(Height::new(0, self.height)?)
    }

    fn host_timestamp(&self) -> Result<Timestamp, ContextError> {
        let host_height = self.host_height()?;
        let host_cons_state = self.host_consensus_state(&host_height)?;
        Ok(host_cons_state.timestamp())
    }

    fn host_consensus_state(
        &self,
        height: &Height,
    ) -> Result<Box<dyn ConsensusState>, ContextError> {
        let index = self.host_consensus_states.len() - 1 - (self.height - height.revision_height());
        Ok(Box::<TmConsensusState>::new(
            self.host_consensus_states
                .get(index)
                .map_err(|_| ClientError::ImplementationSpecific)?
                .unwrap()
                .clone()
                .into(),
        ))
    }

    fn client_counter(&self) -> Result<u64, ContextError> {
        Ok(self.client_counter)
    }

    fn connection_end(&self, conn_id: &IbcConnectionId) -> Result<IbcConnectionEnd, ContextError> {
        Ok(self
            .connections
            .get(conn_id.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(ConnectionError::ConnectionNotFound {
                connection_id: conn_id.clone(),
            })?
            .clone()
            .into())
    }

    fn validate_self_client(
        &self,
        client_state_of_host_on_counterparty: Any,
    ) -> Result<(), ContextError> {
        Ok(())
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        CommitmentPrefix::try_from(b"ibc".to_vec()).unwrap()
    }

    fn connection_counter(&self) -> Result<u64, ContextError> {
        Ok(self.connection_counter)
    }

    fn channel_end(
        &self,
        channel_end_path: &ChannelEndPath,
    ) -> Result<IbcChannelEnd, ContextError> {
        Ok(self
            .channel_ends
            .get(channel_end_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(ChannelError::MissingChannel)?
            .clone()
            .into())
    }

    fn get_next_sequence_send(
        &self,
        seq_send_path: &SeqSendPath,
    ) -> Result<Sequence, ContextError> {
        Ok(self
            .next_sequence_send
            .get(seq_send_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::MissingPacket)?
            .clone()
            .into_inner()
            .into())
    }

    fn get_next_sequence_recv(
        &self,
        seq_recv_path: &SeqRecvPath,
    ) -> Result<Sequence, ContextError> {
        Ok(self
            .next_sequence_recv
            .get(seq_recv_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::MissingPacket)?
            .clone()
            .into_inner()
            .into())
    }

    fn get_next_sequence_ack(&self, seq_ack_path: &SeqAckPath) -> Result<Sequence, ContextError> {
        Ok(self
            .next_sequence_ack
            .get(seq_ack_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::MissingPacket)?
            .clone()
            .into_inner()
            .into())
    }

    fn get_packet_commitment(
        &self,
        commitment_path: &CommitmentPath,
    ) -> Result<PacketCommitment, ContextError> {
        Ok(self
            .commitments
            .get(commitment_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::MissingPacket)?
            .clone()
            .into())
    }

    fn get_packet_receipt(&self, receipt_path: &ReceiptPath) -> Result<Receipt, ContextError> {
        self.receipts
            .get(receipt_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::MissingPacket)?;
        Ok(Receipt::Ok)
    }

    fn get_packet_acknowledgement(
        &self,
        ack_path: &AckPath,
    ) -> Result<AcknowledgementCommitment, ContextError> {
        Ok(self
            .acks
            .get(ack_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::MissingPacket)?
            .clone()
            .into())
    }

    fn client_update_time(
        &self,
        client_id: &IbcClientId,
        height: &Height,
    ) -> Result<Timestamp, ContextError> {
        Ok(self
            .clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .updates
            .get(height.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .ok_or(ClientError::ImplementationSpecific)?
            .0
            .clone()
            .into())
    }

    fn client_update_height(
        &self,
        client_id: &IbcClientId,
        height: &Height,
    ) -> Result<Height, ContextError> {
        self.clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .updates
            .get((*height).into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .ok_or(ClientError::ImplementationSpecific)?
            .1
            .clone()
            .try_into()
            .map_err(|_| ClientError::ImplementationSpecific.into())
    }

    fn channel_counter(&self) -> Result<u64, ContextError> {
        Ok(self.channel_counter)
    }

    fn max_expected_time_per_block(&self) -> Duration {
        Duration::from_secs(8)
    }
}

impl ExecutionContext for Ibc {
    // fn store_client_type(
    //     &mut self,
    //     client_type_path: ClientTypePath,
    //     client_type: ClientType,
    // ) -> Result<(), ContextError> {
    //     self.clients
    //         .entry(client_type_path.0.into())
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .or_insert_default()
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .client_type = client_type.into();
    //     Ok(())
    // }

    fn store_client_state(
        &mut self,
        client_state_path: ClientStatePath,
        client_state: Box<dyn ClientState>,
    ) -> Result<(), ContextError> {
        let tm_client_state = client_state
            .as_any()
            .downcast_ref::<TmClientState>()
            .ok_or(ClientError::ImplementationSpecific)?;
        self.clients
            .entry(client_state_path.0.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .client_state
            .insert((), tm_client_state.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Box<dyn ConsensusState>,
    ) -> Result<(), ContextError> {
        let epoch_height = format!(
            "{}-{}",
            consensus_state_path.epoch, consensus_state_path.height
        );
        let tm_consensus_state = consensus_state
            .as_any()
            .downcast_ref::<TmConsensusState>()
            .ok_or(ClientError::ImplementationSpecific)?;
        self.clients
            .entry(consensus_state_path.client_id.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .consensus_states
            .insert(epoch_height.into(), tm_consensus_state.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?;
        Ok(())
    }

    fn increase_client_counter(&mut self) {
        self.client_counter += 1;
    }

    fn store_update_time(
        &mut self,
        client_id: IbcClientId,
        height: Height,
        timestamp: Timestamp,
    ) -> Result<(), ContextError> {
        self.clients
            .entry(client_id.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .updates
            .entry(height.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .0 = timestamp.into();
        Ok(())
    }

    fn store_update_height(
        &mut self,
        client_id: IbcClientId,
        height: Height,
        host_height: Height,
    ) -> Result<(), ContextError> {
        self.clients
            .entry(client_id.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .updates
            .entry(height.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .1 = host_height.into();
        Ok(())
    }

    fn store_connection(
        &mut self,
        connection_path: &ConnectionPath,
        connection_end: IbcConnectionEnd,
    ) -> Result<(), ContextError> {
        self.connections
            .insert(connection_path.clone().into(), connection_end.into())
            .map_err(|_| ConnectionError::Client(ClientError::ImplementationSpecific))?;
        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        client_connection_path: &ClientConnectionPath,
        conn_id: IbcConnectionId,
    ) -> Result<(), ContextError> {
        self.clients
            .entry(client_connection_path.clone().into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .connections
            .insert(conn_id.into(), ())
            .map_err(|_| ClientError::ImplementationSpecific)?;
        Ok(())
    }

    fn increase_connection_counter(&mut self) {
        self.connection_counter += 1;
    }

    fn store_packet_commitment(
        &mut self,
        commitment_path: &CommitmentPath,
        commitment: PacketCommitment,
    ) -> Result<(), ContextError> {
        self.commitments
            .insert(commitment_path.clone().into(), commitment.into_vec())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_commitment(
        &mut self,
        commitment_path: &CommitmentPath,
    ) -> Result<(), ContextError> {
        self.commitments
            .remove(commitment_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        receipt_path: &ReceiptPath,
        _receipt: Receipt,
    ) -> Result<(), ContextError> {
        self.receipts
            .insert(receipt_path.clone().into(), ())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_packet_acknowledgement(
        &mut self,
        ack_path: &AckPath,
        ack_commitment: AcknowledgementCommitment,
    ) -> Result<(), ContextError> {
        self.acks
            .insert(ack_path.clone().into(), ack_commitment.into_vec())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn delete_packet_acknowledgement(&mut self, ack_path: &AckPath) -> Result<(), ContextError> {
        self.acks
            .remove(ack_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_channel(
        &mut self,
        channel_end_path: &ChannelEndPath,
        channel_end: IbcChannelEnd,
    ) -> Result<(), ContextError> {
        self.channel_ends
            .insert(channel_end_path.clone().into(), channel_end.into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_send(
        &mut self,
        seq_send_path: &SeqSendPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.next_sequence_send
            .insert(seq_send_path.clone().into(), seq.into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_recv(
        &mut self,
        seq_recv_path: &SeqRecvPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.next_sequence_recv
            .insert(seq_recv_path.clone().into(), seq.into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn store_next_sequence_ack(
        &mut self,
        seq_ack_path: &SeqAckPath,
        seq: Sequence,
    ) -> Result<(), ContextError> {
        self.next_sequence_ack
            .insert(seq_ack_path.clone().into(), seq.into())
            .map_err(|_| PacketError::ImplementationSpecific)?;
        Ok(())
    }

    fn increase_channel_counter(&mut self) {
        self.channel_counter += 1;
    }

    fn emit_ibc_event(&mut self, event: IbcEvent) {
        let ctx = match self.context::<Events>() {
            Some(ctx) => ctx,
            None => return,
        };
        let event: tendermint::abci::Event = match event.try_into() {
            Ok(event) => event,
            Err(_) => return,
        };
        let event = match event.try_into() {
            Ok(event) => event,
            Err(_) => return,
        };

        ctx.add(event);
    }

    fn log_message(&mut self, message: String) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use tendermint::Time;

    use crate::Result;

    use super::*;

    fn tm_client_id(n: u64) -> ClientId {
        IbcClientId::new(ClientType::new("07-tendermint".to_string()), n)
            .unwrap()
            .into()
    }

    fn tm_consensus_state(n: i64) -> TmConsensusState {
        TmConsensusState::new(
            vec![0; 32].into(),
            Time::from_unix_timestamp(n * 10_000, 0).unwrap(),
            tendermint::Hash::Sha256([0; 32]),
        )
    }

    fn height(epoch: u64, height: u64) -> Height {
        Height::new(epoch, height).unwrap()
    }

    #[test]
    fn next_prev_consensus_state() -> Result<()> {
        let mut ibc = Ibc::default();

        let consensus_states = &mut ibc
            .clients
            .entry(tm_client_id(123))?
            .or_insert_default()?
            .consensus_states;
        consensus_states.insert(height(1, 10).into(), tm_consensus_state(10).into())?;
        consensus_states.insert(height(1, 12).into(), tm_consensus_state(12).into())?;

        ibc.clients
            .entry(tm_client_id(124))?
            .or_insert_default()?
            .consensus_states
            .insert(height(1, 13).into(), tm_consensus_state(13).into())?;

        assert!(
            ibc.next_consensus_state(&tm_client_id(123), &height(0, 1))?
                .is_none(),
            "next_consensus_state, different epoch"
        );

        assert_eq!(
            ibc.next_consensus_state(&tm_client_id(123), &height(1, 1))?
                .unwrap(),
            tm_consensus_state(10).into_box(),
            "next_consensus_state, skipped heights",
        );

        assert_eq!(
            ibc.next_consensus_state(&tm_client_id(123), &height(1, 10))?
                .unwrap(),
            tm_consensus_state(12).into_box(),
            "next_consensus_state, has value at height",
        );

        assert!(
            ibc.next_consensus_state(&tm_client_id(123), &height(1, 12))?
                .is_none(),
            "next_consensus_state, different client",
        );

        assert!(
            ibc.next_consensus_state(&tm_client_id(123), &height(1, 13))?
                .is_none(),
            "next_consensus_state, no value at height",
        );

        assert_eq!(
            ibc.prev_consensus_state(&tm_client_id(123), &height(1, 13))?
                .unwrap(),
            tm_consensus_state(12).into_box(),
            "prev_consensus_state, has value at height",
        );

        assert_eq!(
            ibc.prev_consensus_state(&tm_client_id(123), &height(1, 12))?
                .unwrap(),
            tm_consensus_state(10).into_box(),
            "prev_consensus_state, skipped heights",
        );

        assert!(
            ibc.prev_consensus_state(&tm_client_id(123), &height(2, 10))?
                .is_none(),
            "prev_consensus_state, different epoch",
        );

        assert!(
            ibc.prev_consensus_state(&tm_client_id(123), &height(1, 1))?
                .is_none(),
            "prev_consensus_state, no value at height",
        );

        assert!(
            ibc.prev_consensus_state(&tm_client_id(124), &height(1, 12))?
                .is_none(),
            "prev_consensus_state, different client",
        );

        Ok(())
    }
}
