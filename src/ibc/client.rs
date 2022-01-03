use super::{Adapter, Ibc, ProtobufAdapter};
use crate::collections::{map::Ref, Deque, Entry, EntryMap, Map};
use crate::state::State;
use ibc::core::ics02_client::client_consensus::AnyConsensusState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::core::ics02_client::error::Error;
use ibc::core::ics24_host::identifier::ClientId;
use ibc::Height;

impl From<crate::Error> for Error {
    fn from(_err: crate::Error) -> Error {
        Error::implementation_specific()
    }
}

#[derive(State)]
pub struct ConsensusStateMap {
    consensus_epochs: Map<u64, Map<u64, ProtobufAdapter<AnyConsensusState>>>,
}

impl ConsensusStateMap {
    pub fn get(&self, height: Height) -> Result<AnyConsensusState, Error> {
        let Height {
            revision_height,
            revision_number,
        } = height;

        let states_for_epoch = self.consensus_epochs.get(revision_number).map(
            |maybe_states| match maybe_states {
                Some(states) => Ok(states),
                None => Err(Error::implementation_specific()),
            },
        )??;

        states_for_epoch
            .get(revision_height)
            .map(|maybe_state| match maybe_state {
                Some(state) => Ok(state.clone()),
                None => Err(Error::implementation_specific()),
            })?
    }

    pub fn insert(
        &mut self,
        height: Height,
        consensus_state: AnyConsensusState,
    ) -> Result<(), Error> {
        let Height {
            revision_height,
            revision_number,
        } = height;

        let mut states_for_epoch = self
            .consensus_epochs
            .entry(revision_number)?
            .or_insert_default()?;

        states_for_epoch.insert(revision_height, consensus_state.into())?;

        Ok(())
    }
}

#[derive(State)]
pub struct ClientStore {
    client_type: Map<Adapter<ClientId>, Adapter<ClientType>>,
    client_state: Map<Adapter<ClientId>, Adapter<AnyClientState>>,
    consensus_state: Map<Adapter<ClientId>, ConsensusStateMap>,
    client_counter: u64,
}

impl ClientKeeper for Ibc {
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), Error> {
        self.client
            .client_type
            .insert(client_id.into(), client_type.into())?;

        Ok(())
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), Error> {
        self.client
            .client_state
            .insert(client_id.into(), client_state.into())?;

        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: Height,
        consensus_state: AnyConsensusState,
    ) -> Result<(), Error> {
        self.client
            .consensus_state
            .entry(client_id.into())?
            .or_insert_default()?
            .insert(height, consensus_state)?;

        Ok(())
    }

    fn increase_client_counter(&mut self) {
        self.client.client_counter += 1;
    }
}

impl ClientReader for Ibc {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, Error> {
        self.client
            .client_type
            .get(client_id.clone().into())
            .map(|entry| match entry {
                Some(v) => Ok(**v),
                None => Err(Error::implementation_specific()),
            })?
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, Error> {
        self.client
            .client_state
            .get(client_id.clone().into())
            .map(|entry| match entry {
                Some(v) => Ok(v.clone().into_inner()),
                None => Err(Error::implementation_specific()),
            })?
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, Error> {
        self.client
            .consensus_state
            .get(client_id.clone().into())
            .map(|maybe_states| match maybe_states {
                Some(states) => Ok(states),
                None => Err(Error::implementation_specific()),
            })??
            .get(height)
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<Option<AnyConsensusState>, Error> {
        todo!()
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<Option<AnyConsensusState>, Error> {
        todo!()
    }

    fn client_counter(&self) -> Result<u64, Error> {
        Ok(self.client.client_counter)
    }
}
