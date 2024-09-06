use std::time::Duration;

use ibc::primitives::Signer;
use ibc_rs::core::channel::types::commitment::{AcknowledgementCommitment, PacketCommitment};
use ibc_rs::core::channel::types::error::{ChannelError, PacketError};
use ibc_rs::core::channel::types::packet::Receipt;
use ibc_rs::core::client::types::error::ClientError;
use ibc_rs::core::commitment_types::commitment::CommitmentPrefix;
#[cfg(feature = "abci")]
use ibc_rs::core::commitment_types::commitment::CommitmentRoot;
use ibc_rs::core::connection::types::error::ConnectionError;
use ibc_rs::core::handler::types::error::ContextError;
use ibc_rs::core::handler::types::events::IbcEvent;
use ibc_rs::core::host::{ExecutionContext, ValidationContext};
use ibc_rs::primitives::Timestamp;

use crate::context::{Context, GetContext};
use crate::plugins::{ChainId as ChainIdCtx, Events, Logs};
use crate::Error;
#[cfg(feature = "abci")]
use crate::{abci::BeginBlock, plugins::BeginBlockCtx};

use super::*;

#[cfg(feature = "abci")]
const MAX_HOST_CONSENSUS_STATES: u64 = 100_000;

#[cfg(feature = "abci")]
impl BeginBlock for Ibc {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> crate::Result<()> {
        self.ctx.begin_block(ctx)
    }
}

#[cfg(feature = "abci")]
impl BeginBlock for IbcContext {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> crate::Result<()> {
        self.height = ctx.height;
        let timestamp = if let Some(ref timestamp) = ctx.header.time {
            tendermint::Time::from_unix_timestamp(timestamp.seconds, timestamp.nanos as u32)
                .map_err(|_| crate::Error::Ibc("Invalid timestamp".to_string()))?
        } else {
            return Err(Error::Tendermint("Missing timestamp".to_string()));
        };

        if ctx.header.app_hash.is_empty() {
            return Ok(());
        }

        let consensus_state = ibc::clients::tendermint::types::ConsensusState::new(
            CommitmentRoot::from_bytes(ctx.header.app_hash.as_slice()),
            timestamp,
            tendermint::Hash::Sha256(
                ctx.header
                    .next_validators_hash
                    .clone()
                    .try_into()
                    .map_err(|_| Error::Tendermint("Invalid hash".to_string()))?,
            ),
        );
        self.host_consensus_states
            .push_back(TmConsensusState::from(consensus_state).into())?;

        while self.host_consensus_states.len() > MAX_HOST_CONSENSUS_STATES {
            self.host_consensus_states.pop_front()?;
        }

        Ok(())
    }
}

impl ValidationContext for IbcContext {
    type V = Self;

    type HostClientState = TmClientState;

    type HostConsensusState = TmConsensusState;

    fn validate_message_signer(&self, signer: &Signer) -> Result<(), ContextError> {
        use crate::plugins::Signer as SignerCtx;
        let ctx = Context::resolve::<SignerCtx>()
            .ok_or_else(|| Error::Signer("Invalid signer".to_string()))
            .map_err(|e| ClientError::InvalidSigner {
                reason: e.to_string(),
            })?;

        let expected_signer = ctx.signer.ok_or_else(|| ClientError::InvalidSigner {
            reason: "Missing signer".to_string(),
        })?;
        let actual_signer: Address =
            signer
                .clone()
                .try_into()
                .map_err(|e: crate::Error| ClientError::InvalidSigner {
                    reason: e.to_string(),
                })?;

        if expected_signer != actual_signer {
            return Err(ClientError::InvalidSigner {
                reason: "Invalid signer".to_string(),
            }
            .into());
        }
        Ok(())
    }

    fn host_height(&self) -> Result<Height, ContextError> {
        let ctx = Context::resolve::<ChainIdCtx>().ok_or_else(|| {
            log::error!("Missing chain ID context");
            ContextError::ClientError(ClientError::ClientSpecific {
                description: "Missing chain ID context".to_string(),
            })
        })?;
        let chain_id = ctx.0.as_str();
        let revision_number = chain_id
            .rsplit_once('-')
            .map(|(_, n)| n.parse::<u64>().unwrap_or(0))
            .unwrap_or(0);

        Ok(Height::new(revision_number, self.height)?)
    }

    fn host_timestamp(&self) -> Result<Timestamp, ContextError> {
        let host_height = self.host_height()?;
        let host_cons_state = self.host_consensus_state(&host_height)?;
        Ok(host_cons_state
            .timestamp()
            .try_into()
            .map_err(|_| ClientError::Other {
                description: "Invalid timestamp".into(),
            })?)
    }

    fn host_consensus_state(
        &self,
        height: &Height,
    ) -> Result<Self::HostConsensusState, ContextError> {
        let index = self.host_consensus_states.len() - 1 - (self.height - height.revision_height());
        Ok(self
            .host_consensus_states
            .get(index)
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get host consensus state".to_string(),
            })?
            .unwrap()
            .clone()
            .try_into()
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get host consensus state".to_string(),
            })?)
    }

    fn client_counter(&self) -> Result<u64, ContextError> {
        Ok(self.client_counter)
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ContextError> {
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
        _client_state_of_host_on_counterparty: Self::HostClientState,
    ) -> Result<(), ContextError> {
        Ok(())
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        CommitmentPrefix::from(b"ibc".to_vec())
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
            .ok_or(PacketError::ImplementationSpecific)?
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
            .ok_or(PacketError::ImplementationSpecific)?
            .clone()
            .into_inner()
            .into())
    }

    fn get_next_sequence_ack(&self, seq_ack_path: &SeqAckPath) -> Result<Sequence, ContextError> {
        Ok(self
            .next_sequence_ack
            .get(seq_ack_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::ImplementationSpecific)?
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
            .ok_or(PacketError::ImplementationSpecific)?
            .clone()
            .into())
    }

    fn get_packet_receipt(&self, receipt_path: &ReceiptPath) -> Result<Receipt, ContextError> {
        self.receipts
            .get(receipt_path.clone().into())
            .map_err(|_| PacketError::ImplementationSpecific)?
            .ok_or(PacketError::PacketReceiptNotFound {
                sequence: receipt_path.sequence,
            })?;
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
            .ok_or(PacketError::PacketAcknowledgementNotFound {
                sequence: ack_path.sequence,
            })?
            .clone()
            .into())
    }

    fn channel_counter(&self) -> Result<u64, ContextError> {
        Ok(self.channel_counter)
    }

    fn max_expected_time_per_block(&self) -> Duration {
        Duration::from_secs(8)
    }

    fn get_client_validation_context(&self) -> &Self::V {
        self
    }
}

impl ExecutionContext for IbcContext {
    type E = Self;

    fn get_client_execution_context(&mut self) -> &mut Self::E {
        self
    }

    fn increase_client_counter(&mut self) -> Result<(), ContextError> {
        self.client_counter += 1;

        Ok(())
    }

    // fn store_update_time(
    //     &mut self,
    //     client_id: IbcClientId,
    //     height: Height,
    //     timestamp: Timestamp,
    // ) -> Result<(), ContextError> {
    //     self.clients
    //         .entry(client_id.into())
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .or_insert_default()
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .updates
    //         .entry(height.into())
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .or_insert_default()
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .0 = timestamp.into();
    //     Ok(())
    // }

    // fn store_update_height(
    //     &mut self,
    //     client_id: IbcClientId,
    //     height: Height,
    //     host_height: Height,
    // ) -> Result<(), ContextError> {
    //     self.clients
    //         .entry(client_id.into())
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .or_insert_default()
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .updates
    //         .entry(height.into())
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .or_insert_default()
    //         .map_err(|_| ClientError::ImplementationSpecific)?
    //         .1 = host_height.into();
    //     Ok(())
    // }

    fn store_connection(
        &mut self,
        connection_path: &ConnectionPath,
        connection_end: ConnectionEnd,
    ) -> Result<(), ContextError> {
        self.connections
            .insert(connection_path.clone().into(), connection_end.into())
            .map_err(|_| {
                ConnectionError::Client(ClientError::ClientSpecific {
                    description: "Unable to store connection".to_string(),
                })
            })?;
        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        client_connection_path: &ClientConnectionPath,
        conn_id: ConnectionId,
    ) -> Result<(), ContextError> {
        self.clients
            .entry(client_connection_path.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to find client connection path".to_string(),
            })?
            .or_insert_default()
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to find client connection path".to_string(),
            })?
            .connections
            .insert(conn_id.into(), ())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to find client connection path".to_string(),
            })?;
        Ok(())
    }

    fn increase_connection_counter(&mut self) -> Result<(), ContextError> {
        self.connection_counter += 1;
        Ok(())
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
            .insert(receipt_path.clone().into(), 1)
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

    fn increase_channel_counter(&mut self) -> Result<(), ContextError> {
        self.channel_counter += 1;

        Ok(())
    }

    fn emit_ibc_event(&mut self, event: IbcEvent) -> Result<(), ContextError> {
        let ctx = match self.context::<Events>() {
            Some(ctx) => ctx,
            None => return Ok(()),
        };
        let mut event: tendermint::abci::Event = match event.try_into() {
            Ok(event) => event,
            Err(_) => return Ok(()),
        };
        for attr in event.attributes.iter_mut() {
            let proto_attr = tendermint::abci::v0_34::EventAttribute {
                key: attr.key_bytes().to_vec(),
                value: attr.value_bytes().to_vec(),
                index: true,
            };
            *attr = tendermint::abci::EventAttribute::V034(proto_attr)
        }

        let mut event: tendermint_proto::v0_34::abci::Event = event.into();

        for attribute in event.attributes.iter_mut() {
            attribute.index = true;
        }

        ctx.add(event);

        Ok(())
    }

    fn log_message(&mut self, message: String) -> Result<(), ContextError> {
        let ctx = match self.context::<Logs>() {
            Some(ctx) => ctx,
            None => return Ok(()),
        };
        ctx.add(message);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tendermint::Time;

    use crate::Result;

    use super::*;
    use ibc::clients::tendermint::types::ConsensusState;
    use ibc::core::client::context::ExtClientValidationContext;

    fn tm_client_id(n: u64) -> ClientIdKey {
        ClientId::new("07-tendermint", n).unwrap().into()
    }

    fn tm_consensus_state(n: i64) -> TmConsensusState {
        let consensus_state = ConsensusState::new(
            vec![0; 32].into(),
            Time::from_unix_timestamp(n * 10_000, 0).unwrap(),
            tendermint::Hash::Sha256([0; 32]),
        );
        TmConsensusState::from(consensus_state)
    }

    fn height(epoch: u64, height: u64) -> Height {
        Height::new(epoch, height).unwrap()
    }

    #[test]
    fn next_prev_consensus_state() -> Result<()> {
        let mut ibc = IbcContext::default();

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
            tm_consensus_state(10).into(),
            "next_consensus_state, skipped heights",
        );

        assert_eq!(
            ibc.next_consensus_state(&tm_client_id(123), &height(1, 10))?
                .unwrap(),
            tm_consensus_state(12).into(),
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
            tm_consensus_state(12).into(),
            "prev_consensus_state, has value at height",
        );

        assert_eq!(
            ibc.prev_consensus_state(&tm_client_id(123), &height(1, 12))?
                .unwrap(),
            tm_consensus_state(10).into(),
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
