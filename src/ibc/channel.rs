use super::encoding::{Adapter, ProtobufAdapter};
use super::{Ibc, Lunchbox};
use crate::call::Call;
use crate::client::Client;
use crate::collections::{Deque, Map};
use crate::encoding::{Decode, Encode, LengthVec};
use crate::query::Query;
use crate::state::State;
use crate::store::{Read, Write};
use ibc::core::ics02_client::client_state::ClientState as ClientStateTrait;
use ibc::core::ics02_client::consensus_state::ConsensusState as ConsensusStateTrait;
use ibc::core::ics02_client::context::ClientReader;
use ibc::core::ics03_connection::connection::ConnectionEnd;
use ibc::core::ics03_connection::context::ConnectionReader;
use ibc::core::ics03_connection::error::Error as ConnectionError;
use ibc::core::ics04_channel::channel::{ChannelEnd, IdentifiedChannelEnd};
use ibc::core::ics04_channel::commitment::{AcknowledgementCommitment, PacketCommitment};
use ibc::core::ics04_channel::context::{ChannelKeeper, ChannelReader};
use ibc::core::ics04_channel::error::Error;
use ibc::core::ics04_channel::packet::{Receipt, Sequence};
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::core::ics24_host::path::{
    AcksPath, ChannelEndsPath, CommitmentsPath, ReceiptsPath, SeqAcksPath, SeqRecvsPath,
    SeqSendsPath,
};
use ibc::core::ics24_host::Path;
use ibc::timestamp::Timestamp;
use ibc::Height;
use ibc_proto::ibc::core::channel::v1::{Channel, IdentifiedChannel, PacketState};
use ripemd::Digest;

impl From<crate::Error> for Error {
    fn from(_err: crate::Error) -> Error {
        dbg!(_err);
        Error::implementation_specific()
    }
}

#[derive(State, Call, Query, Client)]
pub struct ChannelStore {
    channel_counter: u64,
    connection_channels: Map<Adapter<ConnectionId>, Deque<Adapter<(PortId, ChannelId)>>>,
    commitments: Map<Adapter<(PortId, ChannelId)>, Deque<Adapter<PacketState>>>,
    all_channels: Deque<Adapter<(PortId, ChannelId)>>,
    lunchbox: Lunchbox,
    #[call]
    pub height: u64,
}

impl Lunchbox {
    pub fn insert_channel<T: Into<ProtobufAdapter<ChannelEnd>>>(
        &mut self,
        id: (PortId, ChannelId),
        channel_end: T,
    ) -> crate::Result<()> {
        let path = Path::ChannelEnds(ChannelEndsPath(id.0, id.1));
        println!("insert_channel: {}", path);
        let key = path.into_bytes();
        self.0.put(key, channel_end.into().encode()?)
    }

    pub fn insert_seq_send<T: Into<Adapter<Sequence>>>(
        &mut self,
        ids: (PortId, ChannelId),
        seq: T,
    ) -> crate::Result<()> {
        let key = Path::SeqSends(SeqSendsPath(ids.0, ids.1)).into_bytes();
        self.0.put(key, seq.into().encode()?)
    }

    pub fn insert_seq_recv<T: Into<Adapter<Sequence>>>(
        &mut self,
        ids: (PortId, ChannelId),
        seq: T,
    ) -> crate::Result<()> {
        let key = Path::SeqRecvs(SeqRecvsPath(ids.0, ids.1)).into_bytes();
        self.0.put(key, seq.into().encode()?)
    }

    pub fn insert_seq_ack<T: Into<Adapter<Sequence>>>(
        &mut self,
        ids: (PortId, ChannelId),
        seq: T,
    ) -> crate::Result<()> {
        let key = Path::SeqAcks(SeqAcksPath(ids.0, ids.1)).into_bytes();
        self.0.put(key, seq.into().encode()?)
    }

    pub fn insert_packet_commitment(
        &mut self,
        ids: (PortId, ChannelId, Sequence),
        commitment: PacketCommitment,
    ) -> crate::Result<()> {
        let key = Path::Commitments(CommitmentsPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        self.0.put(key, commitment.into_vec())
    }

    pub fn insert_packet_receipt(
        &mut self,
        ids: (PortId, ChannelId, Sequence),
        _receipt: Receipt,
    ) -> crate::Result<()> {
        let key = Path::Receipts(ReceiptsPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        self.0.put(key, vec![0])
    }

    pub fn insert_packet_ack(
        &mut self,
        ids: (PortId, ChannelId, Sequence),
        packet_ack: AcknowledgementCommitment,
    ) -> crate::Result<()> {
        let key = Path::Acks(AcksPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        self.0.put(key, packet_ack.into_vec())
    }

    pub fn read_packet_ack(
        &self,
        ids: (PortId, ChannelId, Sequence),
    ) -> crate::Result<AcknowledgementCommitment> {
        let key = dbg!(Path::Acks(AcksPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        }))
        .into_bytes();

        dbg!(&key);

        self.0
            .get(key.as_slice())?
            .map(|v| v.into())
            .ok_or_else(|| crate::Error::Ibc("Packet ack not found".into()))
    }

    pub fn read_packet_acks(
        &self,
        ids: (PortId, ChannelId),
    ) -> crate::Result<Vec<(Sequence, AcknowledgementCommitment)>> {
        let key = |s: u64| {
            Path::Acks(AcksPath {
                port_id: ids.0.clone(),
                channel_id: ids.1.clone(),
                sequence: s.into(),
            })
            .into_bytes()
        };

        let seqs: Vec<_> = self
            .0
            .range(key(0)..=key(u64::MAX))
            .map(|res| {
                res.map(|(k, v)| {
                    let key: Path = String::from_utf8(k).unwrap().parse().unwrap();
                    let seq = match key {
                        Path::Acks(AcksPath { sequence, .. }) => sequence,
                        _ => panic!("unexpected key"),
                    };
                    (seq, v.into())
                })
            })
            .collect::<crate::Result<_>>()?;

        Ok(seqs)
    }

    pub fn read_packet_receipt(
        &self,
        ids: (PortId, ChannelId, Sequence),
    ) -> Result<Receipt, Error> {
        let key = Path::Receipts(ReceiptsPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        self.0
            .get(&key)
            .map_err(|_| Error::packet_receipt_not_found(ids.2))?
            .map(|_| Receipt::Ok)
            .ok_or_else(|| Error::packet_receipt_not_found(ids.2))
    }

    pub fn delete_packet_commitment(
        &mut self,
        ids: (PortId, ChannelId, Sequence),
    ) -> crate::Result<()> {
        let key = Path::Commitments(CommitmentsPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        self.0.delete(key.as_slice())
    }

    pub fn delete_packet_ack(&mut self, ids: (PortId, ChannelId, Sequence)) -> crate::Result<()> {
        let key = Path::Acks(AcksPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        self.0.delete(key.as_slice())
    }

    pub fn read_packet_commitment(
        &self,
        ids: (PortId, ChannelId, Sequence),
    ) -> crate::Result<PacketCommitment> {
        let key = Path::Commitments(CommitmentsPath {
            port_id: ids.0,
            channel_id: ids.1,
            sequence: ids.2,
        })
        .into_bytes();

        let bytes = self
            .0
            .get(key.as_slice())?
            .ok_or_else(|| crate::Error::Ibc("Commitment not found".into()))?;

        Ok(bytes.into())
    }

    pub fn read_channel(
        &self,
        id: (PortId, ChannelId),
    ) -> crate::Result<ProtobufAdapter<ChannelEnd>> {
        let key = Path::ChannelEnds(ChannelEndsPath(id.0, id.1)).into_bytes();
        let bytes = self
            .0
            .get(&key)?
            .ok_or_else(|| crate::Error::Ibc("Channel not found".into()))?;

        Ok(Decode::decode(bytes.as_slice())?)
    }

    pub fn read_seq_send(&self, id: (PortId, ChannelId)) -> crate::Result<Adapter<Sequence>> {
        let key = Path::SeqSends(SeqSendsPath(id.0, id.1)).into_bytes();
        let bytes = self
            .0
            .get(&key)?
            .ok_or_else(|| crate::Error::Ibc("Sequence not found".into()))?;

        Ok(Decode::decode(bytes.as_slice())?)
    }

    pub fn read_seq_recv(&self, id: (PortId, ChannelId)) -> crate::Result<Adapter<Sequence>> {
        let key = Path::SeqRecvs(SeqRecvsPath(id.0, id.1)).into_bytes();
        let bytes = self
            .0
            .get(&key)?
            .ok_or_else(|| crate::Error::Ibc("Sequence not found".into()))?;

        Ok(Decode::decode(bytes.as_slice())?)
    }

    pub fn read_seq_ack(&self, id: (PortId, ChannelId)) -> crate::Result<Adapter<Sequence>> {
        let key = Path::SeqAcks(SeqAcksPath(id.0, id.1)).into_bytes();
        let bytes = self
            .0
            .get(&key)?
            .ok_or_else(|| crate::Error::Ibc("Sequence not found".into()))?;

        Ok(Decode::decode(bytes.as_slice())?)
    }
}

impl ChannelReader for Ibc {
    fn channel_end(&self, port_id: &PortId, channel_id: &ChannelId) -> Result<ChannelEnd, Error> {
        self.lunchbox
            .read_channel((port_id.clone(), channel_id.clone()))
            .map(|v| v.into_inner())
            .map_err(|_| Error::channel_not_found(port_id.clone(), channel_id.clone()))
    }

    fn connection_end(&self, connection_id: &ConnectionId) -> Result<ConnectionEnd, Error> {
        ConnectionReader::connection_end(self, connection_id).map_err(Error::ics03_connection)
    }

    fn connection_channels(&self, cid: &ConnectionId) -> Result<Vec<(PortId, ChannelId)>, Error> {
        let conn_chans = self
            .channels
            .connection_channels
            .get_or_default(Adapter::new(cid.clone()))?;

        let mut res = vec![];
        for i in 0..conn_chans.len() {
            let chan = conn_chans
                .get(i)?
                .ok_or_else(|| crate::Error::Ibc("Failed to read channel".into()))?;
            res.push(chan.clone().into_inner());
        }

        Ok(res)
    }

    fn client_state(&self, client_id: &ClientId) -> Result<Box<dyn ClientStateTrait>, Error> {
        ConnectionReader::client_state(self, client_id).map_err(Error::ics03_connection)
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<Box<dyn ConsensusStateTrait>, Error> {
        ConnectionReader::client_consensus_state(self, client_id, height)
            .map_err(Error::ics03_connection)
    }

    fn get_next_sequence_send(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<Sequence, Error> {
        Ok(self
            .lunchbox
            .read_seq_send((port_id.clone(), channel_id.clone()))
            .map(|v| v.into_inner())?)
    }

    fn get_next_sequence_recv(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<Sequence, Error> {
        Ok(self
            .lunchbox
            .read_seq_recv((port_id.clone(), channel_id.clone()))
            .map(|v| v.into_inner())?)
    }

    fn get_next_sequence_ack(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<Sequence, Error> {
        Ok(self
            .lunchbox
            .read_seq_ack((port_id.clone(), channel_id.clone()))
            .map(|v| v.into_inner())?)
    }

    fn get_packet_commitment(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        seq: Sequence,
    ) -> Result<PacketCommitment, Error> {
        Ok(self
            .lunchbox
            .read_packet_commitment((port_id.clone(), channel_id.clone(), seq))?)
    }

    fn get_packet_receipt(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        seq: Sequence,
    ) -> Result<Receipt, Error> {
        self.lunchbox
            .read_packet_receipt((port_id.clone(), channel_id.clone(), seq))
    }

    fn get_packet_acknowledgement(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        seq: Sequence,
    ) -> Result<AcknowledgementCommitment, Error> {
        Ok(self
            .lunchbox
            .read_packet_ack((port_id.clone(), channel_id.clone(), seq))?)
    }

    fn hash(&self, value: Vec<u8>) -> Vec<u8> {
        sha2::Sha256::digest(value).to_vec()
    }

    fn host_height(&self) -> Height {
        ClientReader::host_height(self)
    }

    fn channel_counter(&self) -> Result<u64, Error> {
        Ok(self.channels.channel_counter)
    }

    fn host_consensus_state(&self, height: Height) -> Result<Box<dyn ConsensusStateTrait>, Error> {
        Ok(ClientReader::host_consensus_state(self, height)
            .map_err(|_| crate::Error::Ibc("Could not get host consensus state".into()))?)
    }

    fn client_update_height(&self, client_id: &ClientId, height: Height) -> Result<Height, Error> {
        Ok(self.clients.get_update_height(client_id, height)?)
    }

    fn client_update_time(&self, client_id: &ClientId, height: Height) -> Result<Timestamp, Error> {
        Ok(self.clients.get_update_time(client_id, height)?)
    }

    fn max_expected_time_per_block(&self) -> std::time::Duration {
        std::time::Duration::from_secs(8)
    }

    fn pending_host_consensus_state(&self) -> Result<Box<dyn ConsensusStateTrait>, Error> {
        ClientReader::pending_host_consensus_state(self)
            .map_err(ConnectionError::ics02_client)
            .map_err(Error::ics03_connection)
    }
}

impl ChannelKeeper for Ibc {
    fn store_packet_commitment(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        seq: Sequence,
        commitment: PacketCommitment,
    ) -> Result<(), Error> {
        let mut commitments = self
            .channels
            .commitments
            .entry(Adapter::new((port_id.clone(), channel_id.clone())))?
            .or_insert_default()?;

        commitments.push_back(Adapter::new(PacketState {
            port_id: port_id.to_string(),
            channel_id: channel_id.to_string(),
            sequence: seq.into(),
            data: commitment.clone().into_vec(),
        }))?;

        Ok(self
            .lunchbox
            .insert_packet_commitment((port_id, channel_id, seq), commitment)?)
    }

    fn delete_packet_commitment(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
        seq: Sequence,
    ) -> Result<(), Error> {
        Ok(self
            .lunchbox
            .delete_packet_commitment((port_id.clone(), channel_id.clone(), seq))?)
    }

    fn store_packet_receipt(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        seq: Sequence,
        receipt: Receipt,
    ) -> Result<(), Error> {
        Ok(self
            .lunchbox
            .insert_packet_receipt((port_id, channel_id, seq), receipt)?)
    }

    fn store_packet_acknowledgement(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        seq: Sequence,
        ack: AcknowledgementCommitment,
    ) -> Result<(), Error> {
        Ok(self
            .lunchbox
            .insert_packet_ack((port_id, channel_id, seq), ack)?)
    }

    fn delete_packet_acknowledgement(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
        seq: Sequence,
    ) -> Result<(), Error> {
        Ok(self
            .lunchbox
            .delete_packet_ack((port_id.clone(), channel_id.clone(), seq))?)
    }

    fn store_connection_channels(
        &mut self,
        conn_id: ConnectionId,
        port_id: PortId,
        channel_id: ChannelId,
    ) -> Result<(), Error> {
        self.channels
            .all_channels
            .push_back(Adapter::new((port_id.clone(), channel_id.clone())))?;

        Ok(self
            .channels
            .connection_channels
            .entry(Adapter::new(conn_id))?
            .or_insert_default()?
            .push_back(Adapter::new((port_id, channel_id)))?)
    }

    fn store_channel(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        channel_end: ChannelEnd,
    ) -> Result<(), Error> {
        Ok(self
            .lunchbox
            .insert_channel((port_id, channel_id), channel_end)?)
    }

    fn store_next_sequence_send(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), Error> {
        Ok(self.lunchbox.insert_seq_send((port_id, channel_id), seq)?)
    }

    fn store_next_sequence_recv(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), Error> {
        Ok(self.lunchbox.insert_seq_recv((port_id, channel_id), seq)?)
    }

    fn store_next_sequence_ack(
        &mut self,
        port_id: PortId,
        channel_id: ChannelId,
        seq: Sequence,
    ) -> Result<(), Error> {
        Ok(self.lunchbox.insert_seq_ack((port_id, channel_id), seq)?)
    }

    fn increase_channel_counter(&mut self) {
        self.channels.channel_counter += 1;
    }
}

impl ChannelStore {
    #[query]
    pub fn packet_commitments(
        &self,
        ids: Adapter<(PortId, ChannelId)>,
    ) -> crate::Result<Vec<PacketState>> {
        let mut packet_states = vec![];

        let packets = self.commitments.get_or_default(ids.clone())?;
        for i in 0..packets.len() {
            let packet_state = packets
                .get(i)?
                .ok_or_else(|| crate::Error::Ibc("Could not get packet commitment".into()))?;

            if self
                .lunchbox
                .read_packet_commitment((
                    ids.0.clone(),
                    ids.1.clone(),
                    packet_state.sequence.into(),
                ))
                .is_ok()
            {
                packet_states.push(packet_state.clone().into_inner());
            }
        }

        Ok(packet_states)
    }

    #[query]
    pub fn query_connection_channels(
        &self,
        conn_id: Adapter<ConnectionId>,
    ) -> crate::Result<Vec<IdentifiedChannel>> {
        let mut channels = vec![];

        let channels_ = self.connection_channels.get_or_default(conn_id)?;

        for i in 0..channels_.len() {
            let channel_ids = channels_
                .get(i)?
                .ok_or_else(|| crate::Error::Ibc("Could not get channel".into()))?;

            let channel_end = self
                .lunchbox
                .read_channel(channel_ids.clone().into_inner())?
                .into_inner();
            channels.push(
                IdentifiedChannelEnd::new(
                    channel_ids.clone().into_inner().0,
                    channel_ids.clone().into_inner().1,
                    channel_end,
                )
                .into(),
            );
        }

        Ok(channels)
    }

    #[query]
    pub fn query_unreceived_packets(
        &self,
        ids: Adapter<(PortId, ChannelId)>,
        sequences: LengthVec<u16, u64>,
    ) -> Vec<u64> {
        let mut unreceived = vec![];
        for seq in sequences.iter() {
            if self
                .lunchbox
                .read_packet_receipt((ids.0.clone(), ids.1.clone(), (*seq).into()))
                .is_err()
            // TODO: check error variant or return option from read_packet_receipt
            {
                unreceived.push(*seq);
            }
        }

        unreceived
    }

    #[query]
    pub fn query_unreceived_acks(
        &self,
        ids: Adapter<(PortId, ChannelId)>,
        sequences: LengthVec<u16, u64>,
    ) -> Vec<u64> {
        let mut unreceived = vec![];
        for seq in sequences.iter() {
            if self
                .lunchbox
                .read_packet_commitment((ids.0.clone(), ids.1.clone(), (*seq).into()))
                .is_ok()
            // TODO: check error variant or return option from read_packet_receipt
            {
                unreceived.push(*seq);
            }
        }

        unreceived
    }

    #[query]
    pub fn query_packet_acks(
        &self,
        ids: Adapter<(PortId, ChannelId)>,
    ) -> crate::Result<Vec<PacketState>> {
        // TODO: check error variant / use option
        let acks = self
            .lunchbox
            .read_packet_acks((ids.0.clone(), ids.1.clone()))?
            .into_iter()
            .filter_map(|(seq, ack)| {
                let data = ack.into_vec();
                (!data.is_empty()).then(|| PacketState {
                    port_id: ids.0.clone().to_string(),
                    channel_id: ids.1.clone().to_string(),
                    sequence: seq.into(),
                    data,
                })
            })
            .collect();

        Ok(acks)
    }

    #[query]
    pub fn query_channel(&self, ids: Adapter<(PortId, ChannelId)>) -> crate::Result<Channel> {
        self.lunchbox
            .read_channel(ids.into_inner())
            .map(|c| c.into_inner().into())
    }

    #[query]
    pub fn query_channels(&self) -> crate::Result<Vec<IdentifiedChannel>> {
        let mut channels = vec![];

        for i in 0..self.all_channels.len() {
            let channel_ids = self
                .all_channels
                .get(i)?
                .ok_or_else(|| crate::Error::Ibc("Could not get channel".into()))?;
            let channel_end = self
                .lunchbox
                .read_channel(channel_ids.clone().into_inner())?
                .into_inner();
            channels.push(
                IdentifiedChannelEnd::new(
                    channel_ids.clone().into_inner().0,
                    channel_ids.clone().into_inner().1,
                    channel_end,
                )
                .into(),
            );
        }

        Ok(channels)
    }
}

impl Query for Channel {
    type Query = ();
    fn query(&self, _query: Self::Query) -> crate::Result<()> {
        Ok(())
    }
}

impl<T: Clone> Client<T> for Channel {
    type Client = ();
    fn create_client(_parent: T) -> Self::Client {
        unimplemented!()
    }
}
