use std::str::FromStr;

use crate::call::Call;
use crate::client::Client;
use crate::coins::{Address, Amount};
use crate::collections::{Deque, Map};
use crate::encoding::{Decode, Encode, LengthVec};
use crate::query::Query;
use crate::state::State;
use ibc::applications::transfer::context::{
    cosmos_adr028_escrow_address, on_acknowledgement_packet, on_chan_close_confirm,
    on_chan_close_init, on_chan_open_ack, on_chan_open_confirm, on_chan_open_init,
    on_chan_open_try, on_recv_packet, on_timeout_packet, BankKeeper, Ics20Context, Ics20Keeper,
    Ics20Reader,
};
use ibc::applications::transfer::error::Error;
use ibc::applications::transfer::{PrefixedCoin, PrefixedDenom};
use ibc::bigint::U256;
use ibc::core::ics02_client::client_consensus::AnyConsensusState;
use ibc::core::ics02_client::client_state::AnyClientState;
use ibc::core::ics03_connection::connection::ConnectionEnd;
use ibc::core::ics03_connection::error::Error as ConnectionError;
use ibc::core::ics04_channel::channel::{ChannelEnd, Counterparty, Order};
use ibc::core::ics04_channel::commitment::{AcknowledgementCommitment, PacketCommitment};
use ibc::core::ics04_channel::context::{ChannelKeeper, ChannelReader};
use ibc::core::ics04_channel::error::Error as ChannelError;
use ibc::core::ics04_channel::msgs::acknowledgement::Acknowledgement;
use ibc::core::ics04_channel::packet::{Packet, Receipt, Sequence};
use ibc::core::ics04_channel::Version;
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::core::ics26_routing::context::{Module, ModuleOutputBuilder, OnRecvPacketAck};
use ibc::signer::Signer;
use ibc::timestamp::Timestamp;
use ibc::Height;
use ibc_proto::ibc::core::channel::v1::PacketState;
use ripemd::Digest;

use super::{Adapter, Lunchbox};

#[derive(State, Call, Query, Client, Encode, Decode)]
pub struct TransferModule {
    lunchbox: Lunchbox,
    commitments: Map<Adapter<(PortId, ChannelId)>, Deque<Adapter<PacketState>>>,
    #[call]
    pub(super) bank: Bank,
    #[call]
    pub height: u64,
}

unsafe impl Send for TransferModule {}
unsafe impl Sync for TransferModule {}

use ibc::applications::transfer::Amount as IbcAmount;
impl TryFrom<IbcAmount> for Amount {
    type Error = crate::Error;

    fn try_from(amount: IbcAmount) -> crate::Result<Self> {
        let amt: U256 = amount.into();

        Ok(Amount::new(amt.try_into().map_err(|_| {
            crate::Error::Ibc("Coin amount is too large".into())
        })?))
    }
}

impl TryFrom<Signer> for Address {
    type Error = Error;
    fn try_from(signer: Signer) -> Result<Self, Self::Error> {
        signer
            .as_ref()
            .parse()
            .map_err(|_| Error::signer(ibc::signer::SignerError::empty_signer()))
    }
}

impl BankKeeper for TransferModule {
    type AccountId = Address;
    fn burn_coins(&mut self, account: &Self::AccountId, amt: &PrefixedCoin) -> Result<(), Error> {
        let denom: Dynom = amt
            .denom
            .to_string()
            .parse()
            .map_err(|_| Error::invalid_token())?;
        let amount: Amount = amt.amount.try_into().map_err(|_| Error::invalid_token())?;

        self.bank
            .burn(*account, amount, denom)
            .map_err(|_| Error::invalid_token())?;
        Ok(())
    }

    fn mint_coins(&mut self, account: &Self::AccountId, amt: &PrefixedCoin) -> Result<(), Error> {
        let denom: Dynom = amt
            .denom
            .to_string()
            .parse()
            .map_err(|_| Error::invalid_token())?;

        let amount: Amount = amt.amount.try_into().map_err(|_| Error::invalid_token())?;

        self.bank
            .mint(*account, amount, denom)
            .map_err(|_| Error::invalid_token())?;
        Ok(())
    }

    fn send_coins(
        &mut self,
        from: &Self::AccountId,
        to: &Self::AccountId,
        amt: &PrefixedCoin,
    ) -> Result<(), Error> {
        let denom: Dynom = amt
            .denom
            .to_string()
            .parse()
            .map_err(|_| Error::invalid_token())?;
        let amount: Amount = amt.amount.try_into().map_err(|_| Error::invalid_token())?;

        self.bank
            .transfer(*from, *to, amount, denom)
            .map_err(|_| Error::invalid_token())?;
        Ok(())
    }
}

impl Ics20Reader for TransferModule {
    type AccountId = Address;

    fn denom_hash_string(&self, denom: &PrefixedDenom) -> Option<String> {
        Some(denom.to_string())
    }

    fn get_channel_escrow_address(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<<Self as Ics20Reader>::AccountId, Error> {
        // TODO: configurable prefix
        let escrow_addr =
            cosmrs::AccountId::new("nomic", &cosmos_adr028_escrow_address(port_id, channel_id))
                .map_err(|_| Error::parse_account_failure())?
                .to_string()
                .parse()
                .map_err(|_| Error::parse_account_failure())?;

        Ok(escrow_addr)
    }

    fn get_port(&self) -> Result<PortId, Error> {
        Ok("transfer".parse().unwrap())
    }

    fn is_receive_enabled(&self) -> bool {
        true
    }

    fn is_send_enabled(&self) -> bool {
        true
    }
}

impl Ics20Keeper for TransferModule {
    type AccountId = Address;
}

impl Ics20Context for TransferModule {
    type AccountId = Address;
}

impl ChannelKeeper for TransferModule {
    fn store_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        commitment: PacketCommitment,
    ) -> Result<(), ChannelError> {
        let mut commitments = self
            .commitments
            .entry((key.0.clone(), key.1.clone()).into())?
            .or_insert_default()?;
        commitments.push_back(
            PacketState {
                port_id: key.0.to_string(),
                channel_id: key.1.to_string(),
                sequence: key.2.into(),
                data: commitment.clone().into_vec(),
            }
            .into(),
        )?;
        Ok(self.lunchbox.insert_packet_commitment(key, commitment)?)
    }

    fn delete_packet_commitment(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_packet_receipt(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _receipt: Receipt,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_packet_acknowledgement(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
        _ack: AcknowledgementCommitment,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn delete_packet_acknowledgement(
        &mut self,
        _key: (PortId, ChannelId, Sequence),
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_connection_channels(
        &mut self,
        _conn_id: ConnectionId,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_channel(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _channel_end: &ChannelEnd,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_next_sequence_send(
        &mut self,
        ids: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), ChannelError> {
        Ok(self.lunchbox.insert_seq_send(ids, seq)?)
    }

    fn store_next_sequence_recv(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn store_next_sequence_ack(
        &mut self,
        _port_channel_id: (PortId, ChannelId),
        _seq: Sequence,
    ) -> Result<(), ChannelError> {
        unimplemented!()
    }

    fn increase_channel_counter(&mut self) {
        unimplemented!()
    }
}

impl ChannelReader for TransferModule {
    fn channel_end(&self, ids: &(PortId, ChannelId)) -> Result<ChannelEnd, ChannelError> {
        self.lunchbox
            .read_channel(ids.clone())
            .map(|v| v.into_inner())
            .map_err(|_| ChannelError::channel_not_found(ids.0.clone(), ids.1.clone()))
    }

    fn connection_end(&self, conn_id: &ConnectionId) -> Result<ConnectionEnd, ChannelError> {
        Ok(self
            .lunchbox
            .read_connection(conn_id.clone())
            .map_err(|_| {
                ChannelError::ics03_connection(ConnectionError::connection_not_found(
                    conn_id.clone(),
                ))
            })?
            .into_inner())
    }

    fn connection_channels(
        &self,
        _cid: &ConnectionId,
    ) -> Result<Vec<(PortId, ChannelId)>, ChannelError> {
        unimplemented!()
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, ChannelError> {
        Ok(self
            .lunchbox
            .read_client_state(client_id.clone())?
            .into_inner())
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, ChannelError> {
        Ok(self
            .lunchbox
            .read_client_consensus_state(client_id.clone(), height)?
            .into_inner())
    }

    fn get_next_sequence_send(&self, ids: &(PortId, ChannelId)) -> Result<Sequence, ChannelError> {
        Ok(self
            .lunchbox
            .read_seq_send(ids.clone())
            .map(|v| v.into_inner())?)
    }

    fn get_next_sequence_recv(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        unimplemented!()
    }

    fn get_next_sequence_ack(
        &self,
        _port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, ChannelError> {
        unimplemented!()
    }

    fn get_packet_commitment(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<PacketCommitment, ChannelError> {
        unimplemented!()
    }

    fn get_packet_receipt(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<Receipt, ChannelError> {
        unimplemented!()
    }

    fn get_packet_acknowledgement(
        &self,
        _key: &(PortId, ChannelId, Sequence),
    ) -> Result<AcknowledgementCommitment, ChannelError> {
        unimplemented!()
    }

    fn hash(&self, value: Vec<u8>) -> Vec<u8> {
        sha2::Sha256::digest(value).to_vec()
    }

    fn host_height(&self) -> Height {
        Height::new(0, self.height).unwrap()
    }

    fn host_consensus_state(&self, _height: Height) -> Result<AnyConsensusState, ChannelError> {
        unimplemented!()
    }

    fn pending_host_consensus_state(&self) -> Result<AnyConsensusState, ChannelError> {
        unimplemented!()
    }

    fn client_update_time(
        &self,
        _client_id: &ClientId,
        _height: Height,
    ) -> Result<Timestamp, ChannelError> {
        unimplemented!()
    }

    fn client_update_height(
        &self,
        _client_id: &ClientId,
        _height: Height,
    ) -> Result<Height, ChannelError> {
        unimplemented!()
    }

    fn channel_counter(&self) -> Result<u64, ChannelError> {
        unimplemented!()
    }

    fn max_expected_time_per_block(&self) -> std::time::Duration {
        unimplemented!()
    }
}

impl Module for TransferModule {
    fn on_chan_open_init(
        &mut self,
        output: &mut ModuleOutputBuilder,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
    ) -> Result<(), ChannelError> {
        on_chan_open_init(
            self,
            output,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_open_try(
        &mut self,
        output: &mut ModuleOutputBuilder,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
        counterparty_version: &Version,
    ) -> Result<Version, ChannelError> {
        on_chan_open_try(
            self,
            output,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
            counterparty_version,
        )
        .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_open_ack(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &Version,
    ) -> Result<(), ChannelError> {
        on_chan_open_ack(self, output, port_id, channel_id, counterparty_version)
            .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_open_confirm(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_open_confirm(self, output, port_id, channel_id)
            .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_close_init(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_close_init(self, output, port_id, channel_id)
            .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_chan_close_confirm(
        &mut self,
        output: &mut ModuleOutputBuilder,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        on_chan_close_confirm(self, output, port_id, channel_id)
            .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_recv_packet(
        &self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        relayer: &Signer,
    ) -> OnRecvPacketAck {
        on_recv_packet(self, output, packet, relayer)
    }

    fn on_acknowledgement_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> Result<(), ChannelError> {
        on_acknowledgement_packet(self, output, packet, acknowledgement, relayer)
            .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }

    fn on_timeout_packet(
        &mut self,
        output: &mut ModuleOutputBuilder,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), ChannelError> {
        on_timeout_packet(self, output, packet, relayer)
            .map_err(|e: Error| ChannelError::app_module(e.to_string()))
    }
}

impl TransferModule {
    #[query]
    pub fn packet_commitments(
        &self,
        ids: Adapter<(PortId, ChannelId)>,
    ) -> crate::Result<Vec<PacketState>> {
        let mut packet_states = vec![];

        let commitments = self.commitments.get_or_default(ids.clone())?;

        for i in 0..commitments.len() {
            let packet_state = commitments
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
    pub fn escrowed_balance(&self, address: Address, denom: Dynom) -> crate::Result<Amount> {
        let accounts = self.bank.balances.get(denom)?;
        match accounts {
            Some(accounts) => Ok(*accounts.get(address)?.unwrap_or_default()),
            None => Ok(0.into()),
        }
    }
}

#[derive(State, Encode, Decode, Clone, Debug)]
pub struct Dynom(pub LengthVec<u8, u8>);

impl FromStr for Dynom {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes: Vec<u8> = s.as_bytes().into();
        if bytes.len() > u8::MAX as usize {
            return Err(crate::Error::Ibc("Denom name is too long".into()));
        }

        Ok(Self(bytes.try_into()?))
    }
}

#[derive(State, Call, Query, Client, Encode, Decode)]
pub struct Bank {
    #[call]
    pub balances: Map<Dynom, Map<Address, Amount>>,
}

unsafe impl Send for Bank {}
unsafe impl Sync for Bank {}

impl Bank {
    pub fn transfer(
        &mut self,
        from: Address,
        to: Address,
        amount: Amount,
        denom: Dynom,
    ) -> crate::Result<()> {
        let mut denom_balances = self.balances.entry(denom)?.or_default()?;

        let mut sender_balance = denom_balances.entry(from)?.or_default()?;
        *sender_balance = (*sender_balance - amount)?;

        let mut receiver_balance = denom_balances.entry(to)?.or_default()?;
        *receiver_balance = (*receiver_balance + amount)?;

        Ok(())
    }

    pub fn mint(&mut self, to: Address, amount: Amount, denom: Dynom) -> crate::Result<()> {
        let mut denom_balances = self.balances.entry(denom)?.or_default()?;

        let mut receiver_balance = denom_balances.entry(to)?.or_default()?;
        *receiver_balance = (*receiver_balance + amount)?;

        Ok(())
    }

    pub fn burn(&mut self, from: Address, amount: Amount, denom: Dynom) -> crate::Result<()> {
        let mut denom_balances = self.balances.entry(denom)?.or_default()?;

        let mut sender_balance = denom_balances.entry(from)?.or_default()?;
        *sender_balance = (*sender_balance - amount)?;

        Ok(())
    }
}
