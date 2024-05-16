use ibc::{
    clients::tendermint::{
        client_state::ClientState, consensus_state::ConsensusState, context::ValidationContext,
    },
    core::{
        client::{
            context::{ClientExecutionContext, ClientValidationContext},
            types::{error::ClientError, Height},
        },
        handler::types::error::ContextError,
        host::types::{
            identifiers::ClientId,
            path::{ClientConsensusStatePath, ClientStatePath},
        },
    },
    derive::ConsensusState,
    primitives::{proto::Any, Timestamp},
};

use ibc::clients::tendermint::context::{
    ConsensusStateConverter, ValidationContext as TmValidationContext,
};

use crate::encoding::EofTerminatedString;
use ibc::clients::tendermint::client_state::ClientState as TmClientState;
use ibc::clients::tendermint::consensus_state::ConsensusState as TmConsensusState;

use super::{IbcContext, WrappedClientState, WrappedConsensusState};
use std::ops::Bound;

impl ClientValidationContext for IbcContext {
    type ClientStateRef = TmClientState;

    type ConsensusStateRef = AnyConsensusState;

    fn client_state(
        &self,
        client_id: &ibc::core::host::types::identifiers::ClientId,
    ) -> Result<Self::ClientStateRef, ContextError> {
        todo!()
    }

    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::ConsensusStateRef, ContextError> {
        todo!()
    }

    fn client_update_meta(
        &self,
        client_id: &ibc::core::host::types::identifiers::ClientId,
        height: &ibc::core::client::types::Height,
    ) -> Result<(ibc::primitives::Timestamp, ibc::core::client::types::Height), ContextError> {
        todo!()
    }
}

impl ClientExecutionContext for IbcContext {
    type ClientStateMut = TmClientState;

    fn store_client_state(
        &mut self,
        client_state_path: ClientStatePath,
        client_state: Self::ClientStateRef,
    ) -> Result<(), ContextError> {
        self.clients
            .entry(client_state_path.0.into())
            .map_err(|_| ClientError::ImplementationSpecific)?
            .or_insert_default()
            .map_err(|_| ClientError::ImplementationSpecific)?
            .client_state
            .insert(Default::default(), client_state.into())
            .map_err(|_| ClientError::ImplementationSpecific)?;

        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Self::ConsensusStateRef,
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

    fn delete_consensus_state(
        &mut self,
        consensus_state_path: ibc::core::host::types::path::ClientConsensusStatePath,
    ) -> Result<(), ContextError> {
        todo!()
    }

    fn store_update_meta(
        &mut self,
        client_id: ibc::core::host::types::identifiers::ClientId,
        height: ibc::core::client::types::Height,
        host_timestamp: ibc::primitives::Timestamp,
        host_height: ibc::core::client::types::Height,
    ) -> Result<(), ContextError> {
        todo!()
    }

    fn delete_update_meta(
        &mut self,
        client_id: ibc::core::host::types::identifiers::ClientId,
        height: ibc::core::client::types::Height,
    ) -> Result<(), ContextError> {
        todo!()
    }
}

impl TmValidationContext for IbcContext {
    fn host_timestamp(&self) -> Result<Timestamp, ContextError> {
        ValidationContext::host_timestamp(self)
    }
    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: &Height,
    ) -> Result<Option<Self::ConsensusStateRef>, ContextError> {
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
        height: &Height,
    ) -> Result<Option<Self::ConsensusStateRef>, ContextError> {
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

    fn host_height(&self) -> Result<Height, ContextError> {
        todo!()
    }

    fn consensus_state_heights(&self, client_id: &ClientId) -> Result<Vec<Height>, ContextError> {
        todo!()
    }
}

use derive_more::{From, TryInto};
use ibc::clients::tendermint::types::ConsensusState as ConsensusStateType;
#[derive(ConsensusState, From, TryInto)]
pub enum AnyConsensusState {
    Tendermint(ConsensusState),
}

impl From<ConsensusStateType> for AnyConsensusState {
    fn from(value: ConsensusStateType) -> Self {
        AnyConsensusState::Tendermint(value.into())
    }
}

impl TryFrom<AnyConsensusState> for ConsensusStateType {
    type Error = ClientError;

    fn try_from(value: AnyConsensusState) -> Result<Self, Self::Error> {
        match value {
            AnyConsensusState::Tendermint(tm_consensus_state) => {
                Ok(tm_consensus_state.inner().clone())
            }
        }
    }
}

impl From<AnyConsensusState> for Any {
    fn from(value: AnyConsensusState) -> Self {
        match value {
            AnyConsensusState::Tendermint(tm_consensus_state) => tm_consensus_state.into(),
        }
    }
}
