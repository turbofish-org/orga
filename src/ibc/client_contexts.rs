use crate::encoding::EofTerminatedString;

use super::{ConsensusState, IbcContext};
use std::ops::Bound;

use ibc::{
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState, CommonContext,
        ValidationContext as TmValidationContext,
    },
    core::{
        ics02_client::{error::ClientError, ClientExecutionContext},
        ics24_host::{
            identifier::ClientId,
            path::{ClientConsensusStatePath, ClientStatePath},
        },
        timestamp::Timestamp,
        ContextError, ValidationContext,
    },
    Height,
};

impl ClientExecutionContext for IbcContext {
    type ClientValidationContext = Self;
    type AnyClientState = TmClientState;
    type AnyConsensusState = ConsensusState;

    fn store_client_state(
        &mut self,
        client_state_path: ClientStatePath,
        client_state: Self::AnyClientState,
    ) -> Result<(), ContextError> {
        self.clients
            .entry(client_state_path.0.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .client_state
            .insert((), client_state.into())
            .map_err(|_| ClientError::ImplementationSpecific)?;

        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Self::AnyConsensusState,
    ) -> Result<(), ContextError> {
        let epoch_height = format!(
            "{}-{}",
            consensus_state_path.epoch, consensus_state_path.height
        );
        self.clients
            .entry(consensus_state_path.client_id.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .consensus_states
            .insert(epoch_height.into(), consensus_state)
            .map_err(|_| ClientError::ImplementationSpecific)?;

        Ok(())
    }
}

impl TmValidationContext for IbcContext {
    fn host_timestamp(&self) -> Result<Timestamp, ContextError> {
        ValidationContext::host_timestamp(self)
    }
    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: &ibc::Height,
    ) -> Result<Option<Self::AnyConsensusState>, ContextError> {
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
            .map(|res| res.map(|(_, v)| v.clone()))
            .transpose()
            .map_err(|_| ContextError::ClientError(ClientError::ImplementationSpecific))
    }
    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: &ibc::Height,
    ) -> Result<Option<Self::AnyConsensusState>, ContextError> {
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
            .map(|res| res.map(|(_, v)| v.clone()))
            .transpose()
            .map_err(|_| ContextError::ClientError(ClientError::ImplementationSpecific))
    }
}

impl CommonContext for IbcContext {
    type ConversionError = &'static str;
    type AnyConsensusState = ConsensusState;
    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::AnyConsensusState, ContextError> {
        ValidationContext::consensus_state(self, client_cons_state_path)
    }
}
