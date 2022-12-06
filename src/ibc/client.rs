use std::convert::TryInto;
use std::ops::Bound;

use super::{Adapter, Ibc, Lunchbox, ProtobufAdapter};
#[cfg(feature = "abci")]
use crate::abci::BeginBlock;
use crate::call::Call;
use crate::client::Client;
use crate::collections::Map;
use crate::collections::Next;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
#[cfg(feature = "abci")]
use crate::plugins::BeginBlockCtx;
use crate::query::Query;
use crate::state::State;
use crate::store::{Read, Write};
use ibc::clients::ics07_tendermint::consensus_state::ConsensusState;
use ibc::core::ics02_client::client_consensus::AnyConsensusState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::core::ics02_client::error::Error;
use ibc::core::ics23_commitment::commitment::CommitmentRoot;
use ibc::core::ics24_host::identifier::ClientId;
use ibc::core::ics24_host::path::ClientStatePath;
use ibc::core::ics24_host::path::{ClientConsensusStatePath, ClientTypePath};
use ibc::core::ics24_host::Path;
use ibc::timestamp::Timestamp;
use ibc::Height;
use ibc_proto::ibc::core::client::v1::ConsensusStateWithHeight;
use ibc_proto::ibc::core::client::v1::IdentifiedClientState;
use serde::Deserialize;
use serde::Serialize;

impl From<crate::Error> for Error {
    fn from(_err: crate::Error) -> Error {
        dbg!(_err);
        Error::implementation_specific()
    }
}

impl Lunchbox {
    fn insert_client_type<T: Into<Adapter<ClientType>>>(
        &mut self,
        client_id: ClientId,
        client_type: T,
    ) -> crate::Result<()> {
        let key = Path::ClientType(ClientTypePath(client_id)).into_bytes();
        self.0.put(key, client_type.into().encode()?)
    }

    fn insert_client_consensus_state<T: Into<ProtobufAdapter<AnyConsensusState>>>(
        &mut self,
        client_id: ClientId,
        client_height: Height,
        client_consensus_state: T,
    ) -> crate::Result<()> {
        let key = Path::ClientConsensusState(ClientConsensusStatePath {
            client_id,
            epoch: client_height.revision_number(),
            height: client_height.revision_height(),
        })
        .into_bytes();
        self.0.put(key, client_consensus_state.into().encode()?)
    }

    fn insert_client_state<T: Into<ProtobufAdapter<AnyClientState>>>(
        &mut self,
        client_id: ClientId,
        state: T,
    ) -> crate::Result<()> {
        let key = Path::ClientState(ClientStatePath(client_id)).into_bytes();

        self.0.put(key, state.into().encode()?)
    }

    pub fn read_client_state(
        &self,
        client_id: ClientId,
    ) -> crate::Result<ProtobufAdapter<AnyClientState>> {
        let key = Path::ClientState(ClientStatePath(client_id)).into_bytes();
        let bytes = self
            .0
            .get(&key)?
            .ok_or_else(|| crate::Error::Ibc("Client state not found".into()))?;

        Ok(Decode::decode(bytes.as_slice())?)
    }

    pub fn read_client_consensus_state(
        &self,
        client_id: ClientId,
        height: Height,
    ) -> crate::Result<ProtobufAdapter<AnyConsensusState>> {
        let key = Path::ClientConsensusState(ClientConsensusStatePath {
            client_id,
            epoch: height.revision_number(),
            height: height.revision_height(),
        })
        .into_bytes();
        let bytes = self
            .0
            .get(&key)?
            .ok_or_else(|| crate::Error::Ibc("Client consensus state not found".into()))?;

        Ok(Decode::decode(bytes.as_slice())?)
    }
}

#[derive(State, Encode, Decode, Default, Describe)]
pub struct ConsensusStateMap {
    states: Map<Adapter<Height>, ProtobufAdapter<AnyConsensusState>>,
    prev_height: Map<Adapter<Height>, Adapter<Height>>,
    latest_height: Map<u8, Adapter<Height>>,
}

impl ConsensusStateMap {
    fn insert(&mut self, height: Height, consensus_state: AnyConsensusState) -> crate::Result<()> {
        let next_height = self.next_height(height)?;
        if let Some(next_height) = next_height {
            if let Some(mut next_prev_height) = self.prev_height.get_mut(next_height.into())? {
                let old_next_prev_height = next_prev_height.clone();
                *next_prev_height = height.into();
                self.prev_height
                    .insert(height.into(), old_next_prev_height)?;
            }
        } else {
            let old_latest = self.latest_height.get(0)?;
            if let Some(old_latest) = old_latest {
                self.prev_height.insert(height.into(), old_latest.clone())?;
            }
            self.latest_height.insert(0, height.into())?;
        }

        self.states.insert(height.into(), consensus_state.into())
    }

    fn next_height(&self, height: Height) -> crate::Result<Option<Height>> {
        let rb = (Bound::Excluded(Adapter::from(height)), Bound::Unbounded);
        if let Some((next_height, _)) = self.states.range(rb)?.next().transpose()? {
            Ok(Some(next_height.clone().into_inner()))
        } else {
            Ok(None)
        }
    }

    fn next_state(&self, height: Height) -> crate::Result<Option<AnyConsensusState>> {
        let next_height = self.next_height(height)?;
        if let Some(next_height) = next_height {
            Ok(self.states.get(next_height.into())?.map(|v| v.clone()))
        } else {
            Ok(None)
        }
    }

    fn prev_state(&self, height: Height) -> crate::Result<Option<AnyConsensusState>> {
        let prev_height = self.prev_height.get(height.into())?;
        if let Some(prev_height) = prev_height {
            Ok(self.states.get(prev_height.clone())?.map(|v| v.clone()))
        } else {
            Ok(None)
        }
    }

    fn get(&self, height: Height) -> crate::Result<Option<AnyConsensusState>> {
        Ok(self.states.get(height.into())?.map(|v| v.clone()))
    }
}

#[derive(State, Call, Query, Client, Encode, Decode, Default, Serialize, Deserialize)]
pub struct ClientStore {
    #[serde(skip)]
    host_consensus_state: Map<u64, ProtobufAdapter<ConsensusState>>,
    height: u64,
    #[serde(skip)]
    client_type: Map<Adapter<ClientId>, Adapter<ClientType>>,
    #[serde(skip)]
    client_state: Map<Adapter<ClientId>, ProtobufAdapter<AnyClientState>>,
    #[serde(skip)]
    client_update_time: Map<Adapter<(ClientId, Height)>, Adapter<Timestamp>>,
    client_consensus_state: Map<Adapter<ClientId>, ConsensusStateMap>,
    #[serde(skip)]
    client_update_height: Map<Adapter<(ClientId, Height)>, Adapter<Height>>,
    client_counter: u64,
}

impl ClientKeeper for Ibc {
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), Error> {
        // println!("store client type");
        self.clients
            .client_type
            .insert(client_id.clone().into(), client_type.into())?;

        self.lunchbox.insert_client_type(client_id, client_type)?;

        Ok(())
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), Error> {
        // println!(
        //     "store client state. client_id: {:?}, state: {:?}",
        //     client_id, client_state
        // );

        self.clients
            .client_state
            .insert(client_id.clone().into(), client_state.clone().into())?;

        self.lunchbox.insert_client_state(client_id, client_state)?;

        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: Height,
        consensus_state: AnyConsensusState,
    ) -> Result<(), Error> {
        // println!(
        //     "store consensus state for client: {:?}, height: {:?}",
        //     client_id, height
        // );

        self.clients
            .client_consensus_state
            .entry(client_id.clone().into())?
            .or_insert_default()?
            .insert(height, consensus_state.clone())?;

        self.lunchbox
            .insert_client_consensus_state(client_id, height, consensus_state)?;

        Ok(())
    }

    fn store_update_time(
        &mut self,
        client_id: ClientId,
        height: Height,
        timestamp: Timestamp,
    ) -> Result<(), Error> {
        // println!("store update time");
        self.clients
            .client_update_time
            .insert((client_id, height).into(), timestamp.into())?;

        Ok(())
    }

    fn store_update_height(
        &mut self,
        client_id: ClientId,
        height: Height,
        host_height: Height,
    ) -> Result<(), Error> {
        // println!("store update height");
        self.clients
            .client_update_height
            .insert((client_id, height).into(), host_height.into())?;

        Ok(())
    }

    fn increase_client_counter(&mut self) {
        // println!("increase client counter");
        self.clients.client_counter += 1;
    }
}

impl ClientReader for Ibc {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, Error> {
        self.clients
            .client_type
            .get(client_id.clone().into())
            .map(|entry| match entry {
                Some(v) => Ok(**v),
                None => Err(Error::implementation_specific()),
            })?
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, Error> {
        // println!("reading client state for client_id: {:?}", client_id);
        self.clients
            .client_state
            .get(client_id.clone().into())
            .map(|entry| match entry {
                Some(v) => Ok(v.clone()),
                None => Err(Error::implementation_specific()),
            })?
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, Error> {
        // println!(
        //     "reading client consensus state for id: {:?}, height: {:?}",
        //     client_id, height
        // );
        self.clients
            .client_consensus_state
            .get(client_id.clone().into())?
            .ok_or_else(|| Error::client_not_found(client_id.clone()))?
            .get(height)?
            .ok_or_else(|| Error::consensus_state_not_found(client_id.clone(), height))
    }

    fn next_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<Option<AnyConsensusState>, Error> {
        Ok(self
            .clients
            .client_consensus_state
            .get(client_id.clone().into())?
            .ok_or_else(|| Error::client_not_found(client_id.clone()))?
            .next_state(height)?)
    }

    fn prev_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<Option<AnyConsensusState>, Error> {
        Ok(self
            .clients
            .client_consensus_state
            .get(client_id.clone().into())?
            .ok_or_else(|| Error::client_not_found(client_id.clone()))?
            .prev_state(height)?)
    }

    fn client_counter(&self) -> Result<u64, Error> {
        Ok(self.clients.client_counter)
    }

    fn host_height(&self) -> Height {
        Height::new(0, self.clients.height).unwrap()
    }

    fn host_consensus_state(&self, height: Height) -> Result<AnyConsensusState, Error> {
        let consensus_state = self
            .clients
            .host_consensus_state
            .get(height.revision_height())?
            .ok_or_else(|| Error::missing_local_consensus_state(height))?
            .clone();

        Ok(AnyConsensusState::Tendermint(consensus_state))
    }

    fn pending_host_consensus_state(&self) -> Result<AnyConsensusState, Error> {
        let consensus_state = self.host_consensus_state(self.host_height())?;

        Ok(consensus_state)
    }
}

#[cfg(feature = "abci")]
impl BeginBlock for ClientStore {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> crate::Result<()> {
        self.height = ctx.height;
        let time: tendermint::Time = ctx
            .header
            .time
            .clone()
            .ok_or_else(|| crate::Error::Ibc("No timestamp on header".to_string()))?
            .try_into()
            .map_err(|_| crate::Error::Ibc("Invalid timestamp".to_string()))?;

        let next_vals_hash = ctx
            .header
            .next_validators_hash
            .clone()
            .try_into()
            .map_err(|_| crate::Error::Ibc("Invalid next validators hash".to_string()))?;

        let consensus_state = ConsensusState::new(
            CommitmentRoot::from_bytes(ctx.header.app_hash.as_slice()),
            time,
            next_vals_hash,
        );
        self.host_consensus_state
            .insert(self.height, consensus_state.into())
    }
}

impl Next for Adapter<ClientId> {
    fn next(&self) -> Option<Self> {
        let client_type = ClientType::Tendermint;
        let counter = self
            .inner
            .as_str()
            .strip_prefix(format!("{}-", client_type.as_str()).as_str())
            .unwrap()
            .parse::<u64>()
            .unwrap();

        if counter == u64::MAX {
            return None;
        }

        let new_client_id = ClientId::new(client_type, counter + 1).unwrap();

        Some(new_client_id.into())
    }
}

impl Next for Adapter<Height> {
    fn next(&self) -> Option<Self> {
        // TODO: support epochs
        Some(self.increment().into())
    }
}

impl ClientStore {
    pub fn get_update_time(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> crate::Result<Timestamp> {
        self.client_update_time
            .get((client_id.clone(), height).into())?
            .map(|entry| *(entry.clone()))
            .ok_or_else(|| crate::Error::Ibc("Client update time not found".to_string()))
    }

    pub fn get_update_height(&self, client_id: &ClientId, height: Height) -> crate::Result<Height> {
        self.client_update_height
            .get((client_id.clone(), height).into())?
            .map(|entry| *(entry.clone()))
            .ok_or_else(|| crate::Error::Ibc("Client update height not found".to_string()))
    }
}

// Call and query methods
impl ClientStore {
    #[query]
    pub fn query_client_states(&self) -> crate::Result<Vec<IdentifiedClientState>> {
        println!("querying client states in ibc module");
        let mut states = vec![];

        for entry in self.client_state.iter()? {
            let (client_id, state) = entry?;

            states.push(IdentifiedClientState {
                client_id: client_id.clone().as_str().to_string(),
                client_state: Some(state.clone().into()),
            });
        }

        Ok(states)
    }

    #[query]
    pub fn query_consensus_states(
        &self,
        client_id: Adapter<ClientId>,
    ) -> crate::Result<Vec<ConsensusStateWithHeight>> {
        // println!("querying consensus states in ibc module");
        let mut states = vec![];
        let client = self
            .client_consensus_state
            .get(client_id)?
            .ok_or_else(|| crate::Error::Ibc("Client not found".to_string()))?;

        let mut latest_height = client.latest_height.get(0)?;

        while let Some(height) = &latest_height {
            let height = (*height).clone().into_inner();
            let consensus_state = client
                .get(height)?
                .ok_or_else(|| crate::Error::Ibc("Failed reading consensus state".into()))?;

            states.push(ConsensusStateWithHeight {
                height: Some(height.into()),
                consensus_state: Some(consensus_state.into()),
            });

            latest_height = client.prev_height.get(height.into())?;
        }

        Ok(states)
    }
}
