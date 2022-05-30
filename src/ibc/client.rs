use std::convert::TryInto;

use super::{Adapter, Ibc, ProtobufAdapter};
use crate::abci::BeginBlock;
use crate::collections::{map::Ref, Deque, Entry, EntryMap, Map};
use crate::plugins::BeginBlockCtx;
use crate::state::State;
use ibc::clients::ics07_tendermint::consensus_state::ConsensusState;
use ibc::core::ics02_client::client_consensus::AnyConsensusState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics02_client::context::{ClientKeeper, ClientReader};
use ibc::core::ics02_client::error::Error;
use ibc::core::ics23_commitment::commitment::CommitmentRoot;
use ibc::core::ics24_host::identifier::ClientId;
use ibc::timestamp::Timestamp;
use ibc::Height;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState as TmConsensusState;

impl From<crate::Error> for Error {
    fn from(_err: crate::Error) -> Error {
        Error::implementation_specific()
    }
}

// #[derive(State)]
// pub struct ConsensusStateMap {
//     consensus_epochs: Map<u64, Map<u64, ProtobufAdapter<AnyConsensusState>>>,
// }

// impl ConsensusStateMap {
//     pub fn get(&self, height: Height) -> Result<AnyConsensusState, Error> {
//         let Height {
//             revision_height,
//             revision_number,
//         } = height;

//         let states_for_epoch = self.consensus_epochs.get(revision_number).map(
//             |maybe_states| match maybe_states {
//                 Some(states) => Ok(states),
//                 None => Err(Error::implementation_specific()),
//             },
//         )??;

//         states_for_epoch
//             .get(revision_height)
//             .map(|maybe_state| match maybe_state {
//                 Some(state) => Ok(state.clone()),
//                 None => Err(Error::implementation_specific()),
//             })?
//     }

//     pub fn insert(
//         &mut self,
//         height: Height,
//         consensus_state: AnyConsensusState,
//     ) -> Result<(), Error> {
//         let Height {
//             revision_height,
//             revision_number,
//         } = height;

//         let mut states_for_epoch = self
//             .consensus_epochs
//             .entry(revision_number)?
//             .or_insert_default()?;

//         states_for_epoch.insert(revision_height, consensus_state.into())?;

//         Ok(())
//     }
// }

#[derive(State)]
pub struct ClientStore {
    host_consensus_state: Map<u64, ProtobufAdapter<ConsensusState, TmConsensusState>>,
    height: u64,
    client_type: Map<Adapter<ClientId>, Adapter<ClientType>>,
    client_state: Map<Adapter<ClientId>, Adapter<AnyClientState>>,
    client_update_time: Map<Adapter<(ClientId, Height)>, Adapter<Timestamp>>,
    client_consensus_state:
        Map<Adapter<(ClientId, Height)>, ProtobufAdapter<AnyConsensusState, Any>>,
    client_height: Map<Adapter<(ClientId, Height)>, Adapter<Height>>,
    client_counter: u64,
}

impl ClientKeeper for Ibc {
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), Error> {
        println!("store client type");
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
        println!(
            "store client state. client_id: {:?}, state: {:?}",
            client_id, client_state
        );
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
        // self.client
        //     .consensus_state
        //     .entry(client_id.into())?
        //     .or_insert_default()?
        //     .insert(height, consensus_state)?;
        println!("store consensus state");
        self.client
            .client_consensus_state
            .insert((client_id, height).into(), consensus_state.into())?;

        Ok(())
    }

    fn store_update_time(
        &mut self,
        client_id: ClientId,
        height: Height,
        timestamp: Timestamp,
    ) -> Result<(), Error> {
        println!("store update time");
        self.client
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
        println!("store update height");
        self.client
            .client_height
            .insert((client_id, height).into(), host_height.into())?;

        Ok(())
    }

    fn increase_client_counter(&mut self) {
        println!("increase client counter");
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
        println!("reading client state for client_id: {:?}", client_id);
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
        // self.client
        //     .consensus_state
        //     .get(client_id.clone().into())
        //     .map(|maybe_states| match maybe_states {
        //         Some(states) => Ok(states),
        //         None => Err(Error::implementation_specific()),
        //     })??
        //     .get(height)
        todo!()
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

    fn host_height(&self) -> Height {
        Height::new(0, self.client.height)
    }

    fn host_consensus_state(&self, height: Height) -> Result<AnyConsensusState, Error> {
        let consensus_state = self
            .client
            .host_consensus_state
            .get(height.revision_height)?
            .unwrap() // TODO: handle None
            .clone();

        Ok(AnyConsensusState::Tendermint(consensus_state))
    }

    fn pending_host_consensus_state(&self) -> Result<AnyConsensusState, Error> {
        let consensus_state = self.host_consensus_state(self.host_height())?;

        Ok(consensus_state)
    }
}

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

impl ClientStore {
    pub fn query_client_states(&self) -> crate::Result<()> {
        Ok(())
    }
}
