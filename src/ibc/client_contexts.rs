use ibc::clients::tendermint::types::TENDERMINT_CONSENSUS_STATE_TYPE_URL;
use ibc::{
    clients::tendermint::consensus_state::ConsensusState,
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

use crate::encoding::EofTerminatedString;
use ibc::clients::tendermint::client_state::ClientState as TmClientState;
use ibc::core::client::context::ExtClientValidationContext as TmValidationContext;

use super::{IbcContext, WrappedConsensusState};
use ibc::core::host::ValidationContext;
use std::ops::Bound;

impl ClientValidationContext for IbcContext {
    type ClientStateRef = TmClientState;

    type ConsensusStateRef = AnyConsensusState;

    fn client_state(
        &self,
        client_id: &ibc::core::host::types::identifiers::ClientId,
    ) -> Result<Self::ClientStateRef, ContextError> {
        Ok(self
            .clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get client state".to_string(),
            })?
            .ok_or_else(|| ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .client_state
            .get(Default::default())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get client state".to_string(),
            })?
            .ok_or(ClientError::ClientSpecific {
                description: "Unable to get client state".to_string(),
            })?
            .clone()
            .into())
    }

    fn consensus_state(
        &self,
        client_cons_state_path: &ClientConsensusStatePath,
    ) -> Result<Self::ConsensusStateRef, ContextError> {
        let height = Height::new(
            client_cons_state_path.revision_number,
            client_cons_state_path.revision_height,
        )
        .map_err(|_| ClientError::InvalidHeight)?;

        let client = self
            .clients
            .get(client_cons_state_path.client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_cons_state_path.client_id.clone(),
            })?;

        let consensus_state = client
            .consensus_states
            .get(height.into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?
            .ok_or(ClientError::ConsensusStateNotFound {
                client_id: client_cons_state_path.client_id.clone(),
                height,
            })?;

        Ok(consensus_state.inner.clone().into())
    }

    fn client_update_meta(
        &self,
        client_id: &ibc::core::host::types::identifiers::ClientId,
        height: &ibc::core::client::types::Height,
    ) -> Result<(ibc::primitives::Timestamp, ibc::core::client::types::Height), ContextError> {
        let client = self
            .clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?
            .ok_or(ClientError::UpdateMetaDataNotFound {
                client_id: client_id.clone(),
                height: *height,
            })?;
        let metadata = client
            .updates
            .get((*height).into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get update metadata".to_string(),
            })?
            .ok_or(ClientError::UpdateMetaDataNotFound {
                client_id: client_id.clone(),
                height: *height,
            })?;

        Ok((
            metadata.0.clone().into(),
            metadata
                .1
                .clone()
                .try_into()
                .map_err(|_| ClientError::InvalidHeight)?,
        ))
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
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to store client state".to_string(),
            })?
            .or_insert_default()
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to store client state".to_string(),
            })?
            .client_state
            .insert(Default::default(), client_state.into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to store client state".to_string(),
            })?;

        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        consensus_state_path: ClientConsensusStatePath,
        consensus_state: Self::ConsensusStateRef,
    ) -> Result<(), ContextError> {
        let epoch_height = format!(
            "{}-{}",
            consensus_state_path.revision_number, consensus_state_path.revision_height
        );
        self.clients
            .entry(consensus_state_path.client_id.into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to store consensus state".to_string(),
            })?
            .or_insert_default()
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to store consensus state".to_string(),
            })?
            .consensus_states
            .insert(
                epoch_height.into(),
                WrappedConsensusState {
                    inner: ConsensusState::try_from(consensus_state).map_err(|_| {
                        ClientError::ClientSpecific {
                            description: "Failed to store consensus state".to_string(),
                        }
                    })?,
                },
            )
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to store consensus state".to_string(),
            })?;

        Ok(())
    }

    fn delete_consensus_state(
        &mut self,
        consensus_state_path: ibc::core::host::types::path::ClientConsensusStatePath,
    ) -> Result<(), ContextError> {
        self.clients
            .get_mut(consensus_state_path.client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: consensus_state_path.client_id.clone(),
            })?
            .consensus_states
            .remove(consensus_state_path.revision_height.to_string().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to delete consensus state".to_string(),
            })?;

        Ok(())
    }

    fn store_update_meta(
        &mut self,
        client_id: ibc::core::host::types::identifiers::ClientId,
        height: ibc::core::client::types::Height,
        host_timestamp: ibc::primitives::Timestamp,
        host_height: ibc::core::client::types::Height,
    ) -> Result<(), ContextError> {
        let mut client = self
            .clients
            .get_mut(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get client".to_string(),
            })?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?;

        client
            .updates
            .insert(height.into(), (host_timestamp.into(), host_height.into()))
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to store update metadata".to_string(),
            })?;

        Ok(())
    }

    fn delete_update_meta(
        &mut self,
        client_id: ibc::core::host::types::identifiers::ClientId,
        height: ibc::core::client::types::Height,
    ) -> Result<(), ContextError> {
        let mut client = self
            .clients
            .get_mut(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get client".to_string(),
            })?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?;

        client
            .updates
            .remove(height.into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to store update metadata".to_string(),
            })?;

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
        height: &Height,
    ) -> Result<Option<Self::ConsensusStateRef>, ContextError> {
        let end_height = Height::new(height.revision_number() + 1, 1).map_err(|_| {
            ClientError::ClientSpecific {
                description: "Failed to get height".to_string(),
            }
        })?;
        let consensus_state = self
            .clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to get client".to_string(),
            })?
            .ok_or_else(|| ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .consensus_states
            .range((
                Bound::<EofTerminatedString>::Excluded((*height).into()),
                Bound::Excluded(end_height.into()),
            ))
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to get bound".to_string(),
            })?
            .next()
            .map(|res| res.map(|(_, v)| v.clone()))
            .transpose()
            .map_err(|e| {
                ContextError::ClientError(ClientError::ClientSpecific {
                    description: e.to_string(),
                })
            })?;

        if let Some(consensus_state) = consensus_state {
            Ok(Some(AnyConsensusState::Tendermint(
                consensus_state.inner.clone(),
            )))
        } else {
            Ok(None)
        }
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: &Height,
    ) -> Result<Option<Self::ConsensusStateRef>, ContextError> {
        let end_height =
            Height::new(height.revision_number(), 1).map_err(|_| ClientError::ClientSpecific {
                description: "Failed to get height".to_string(),
            })?;
        let consensus_state = self
            .clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to get client".to_string(),
            })?
            .ok_or_else(|| ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?
            .consensus_states
            .range((
                Bound::<EofTerminatedString>::Included(end_height.into()),
                Bound::Excluded((*height).into()),
            ))
            .map_err(|_| ClientError::ClientSpecific {
                description: "Failed to get bounds".to_string(),
            })?
            .next_back()
            .map(|res| res.map(|(_, v)| v.clone()))
            .transpose()
            .map_err(|e| {
                ContextError::ClientError(ClientError::ClientSpecific {
                    description: e.to_string(),
                })
            })?;

        if let Some(consensus_state) = consensus_state {
            Ok(Some(AnyConsensusState::Tendermint(
                consensus_state.inner.clone(),
            )))
        } else {
            Ok(None)
        }
    }

    fn host_height(&self) -> Result<Height, ContextError> {
        ValidationContext::host_height(self)
    }

    fn consensus_state_heights(&self, client_id: &ClientId) -> Result<Vec<Height>, ContextError> {
        let client = self
            .clients
            .get(client_id.clone().into())
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?
            .ok_or(ClientError::ClientStateNotFound {
                client_id: client_id.clone(),
            })?;

        let mut heights = Vec::new();
        for entry in client
            .consensus_states
            .iter()
            .map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?
        {
            let (k, _) = entry.map_err(|_| ClientError::ClientSpecific {
                description: "Unable to get consensus state".to_string(),
            })?;
            let height = k
                .parse::<Height>()
                .map_err(|_| ClientError::InvalidHeight)?;

            heights.push(height);
        }

        Ok(heights)
    }
}

use derive_more::{From, TryInto};
use ibc::clients::tendermint::types::ConsensusState as ConsensusStateType;
#[derive(ConsensusState, From, TryInto, Debug, PartialEq)]
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

impl TryFrom<Any> for AnyConsensusState {
    type Error = ClientError;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        match value.type_url.as_str() {
            TENDERMINT_CONSENSUS_STATE_TYPE_URL => {
                Ok(AnyConsensusState::Tendermint(value.try_into()?))
            }
            _ => Err(ClientError::Other {
                description: "Unknown consensus state type".into(),
            }),
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
