use super::Ibc;
use crate::{
    coins::{Address, Amount},
    collections::Map,
    encoding::LengthVec,
    orga,
};
use cosmrs::AccountId;
use ibc::{
    applications::transfer::{
        context::{
            cosmos_adr028_escrow_address, TokenTransferExecutionContext,
            TokenTransferValidationContext,
        },
        error::TokenTransferError,
        PrefixedCoin, PrefixedDenom, VERSION,
    },
    core::{
        ics04_channel::{
            channel::{Counterparty, Order},
            error::{ChannelError, PacketError},
            packet::{Acknowledgement, Packet},
            Version as ChannelVersion,
        },
        ics24_host::identifier::{ChannelId, ConnectionId, PortId},
        router::{Module, ModuleExtras},
    },
    Signer,
};
const ACCOUNT_PREFIX: &str = "nomic"; // TODO: configurable prefix

#[orga]
pub struct Transfer {
    accounts: Map<Denom, Map<Address, Amount>>,
}

impl TokenTransferValidationContext for Ibc {
    type AccountId = Address;

    fn get_port(&self) -> Result<ibc::core::ics24_host::identifier::PortId, TokenTransferError> {
        Ok(PortId::transfer())
    }

    fn get_escrow_account(
        &self,
        port_id: &ibc::core::ics24_host::identifier::PortId,
        channel_id: &ChannelId,
    ) -> Result<Self::AccountId, TokenTransferError> {
        let account_id = AccountId::new(
            ACCOUNT_PREFIX,
            &cosmos_adr028_escrow_address(port_id, channel_id),
        )
        .map_err(|_| TokenTransferError::ParseAccountFailure)?;
        account_id
            .to_string()
            .parse()
            .map_err(|_| TokenTransferError::ParseAccountFailure)
    }

    fn can_send_coins(&self) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn can_receive_coins(&self) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn send_coins_validate(
        &self,
        _from_account: &Self::AccountId,
        _to_account: &Self::AccountId,
        _coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn mint_coins_validate(
        &self,
        _account: &Self::AccountId,
        _coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn burn_coins_validate(
        &self,
        _account: &Self::AccountId,
        _coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        Ok(())
    }
}

#[orga]
// TODO: transparent state, simple type, and/or no-version
#[derive(Clone, Debug)]
pub struct Denom {
    pub inner: LengthVec<u8, u8>,
}

impl TryFrom<PrefixedDenom> for Denom {
    type Error = crate::Error;

    fn try_from(value: PrefixedDenom) -> crate::Result<Self> {
        value.to_string().try_into()
    }
}

impl TryFrom<String> for Denom {
    type Error = crate::Error;

    fn try_from(value: String) -> crate::Result<Self> {
        let bytes = value.as_bytes().to_vec();
        Ok(Self {
            inner: bytes.try_into()?,
        })
    }
}

impl From<&'static str> for Denom {
    fn from(value: &'static str) -> Self {
        let bytes = value.as_bytes().to_vec();
        Self {
            inner: bytes.try_into().unwrap(),
        }
    }
}

impl TryFrom<ibc::applications::transfer::Amount> for Amount {
    type Error = crate::Error;

    fn try_from(value: ibc::applications::transfer::Amount) -> crate::Result<Self> {
        // TODO: either take dependency on `primitive_types` to get U256, or get
        // try_into<u64> from ibc-rs `amount` type. should not need to use
        // string parsing here.
        let amount = value.to_string();
        let amount = amount.parse::<u64>()?;

        Ok(amount.into())
    }
}

impl From<crate::Error> for TokenTransferError {
    fn from(_: crate::Error) -> Self {
        TokenTransferError::InvalidToken
    }
}

impl TokenTransferExecutionContext for Ibc {
    fn burn_coins_execute(
        &mut self,
        account: &Self::AccountId,
        coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let denom: Denom = coin.denom.clone().try_into()?;
        let amount: Amount = coin.amount.try_into()?;

        let mut denom_balances = self.transfer.accounts.entry(denom)?.or_default()?;

        let mut account_balance = denom_balances.entry(*account)?.or_default()?;
        *account_balance = (*account_balance - amount).result()?;

        Ok(())
    }

    fn send_coins_execute(
        &mut self,
        from: &Self::AccountId,
        to: &Self::AccountId,
        coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let denom: Denom = coin.denom.clone().try_into()?;
        let amount: Amount = coin.amount.try_into()?;

        let mut denom_balances = self.transfer.accounts.entry(denom)?.or_default()?;

        let mut sender_balance = denom_balances.entry(*from)?.or_default()?;
        *sender_balance = (*sender_balance - amount).result()?;

        let mut receiver_balance = denom_balances.entry(*to)?.or_default()?;
        *receiver_balance = (*receiver_balance + amount).result()?;

        Ok(())
    }

    fn mint_coins_execute(
        &mut self,
        account: &Self::AccountId,
        coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let denom: Denom = coin.denom.clone().try_into()?;
        let amount: Amount = coin.amount.try_into()?;

        let mut denom_balances = self.transfer.accounts.entry(denom)?.or_default()?;

        let mut receiver_balance = denom_balances.entry(*account)?.or_default()?;
        *receiver_balance = (*receiver_balance + amount).result()?;

        Ok(())
    }
}

use ibc::applications::transfer::context::*;
impl Module for Ibc {
    fn on_chan_open_init_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<ChannelVersion, ChannelError> {
        on_chan_open_init_validate(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })?;
        Ok(ChannelVersion::new(VERSION.to_string()))
    }

    fn on_chan_open_init_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_init_execute(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })
    }

    fn on_chan_open_try_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &ChannelVersion,
    ) -> Result<ChannelVersion, ChannelError> {
        on_chan_open_try_validate(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })?;
        Ok(ChannelVersion::new(VERSION.to_string()))
    }

    fn on_chan_open_try_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &ChannelVersion,
    ) -> Result<(ModuleExtras, ChannelVersion), ChannelError> {
        on_chan_open_try_execute(
            self,
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
        .map_err(|e: TokenTransferError| ChannelError::AppModule {
            description: e.to_string(),
        })
    }

    fn on_recv_packet_execute(
        &mut self,
        packet: &Packet,
        _relayer: &Signer,
    ) -> (ModuleExtras, Acknowledgement) {
        on_recv_packet_execute(self, packet)
    }

    fn on_acknowledgement_packet_validate(
        &self,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_acknowledgement_packet_validate(self, packet, acknowledgement, relayer).map_err(
            |e: TokenTransferError| PacketError::AppModule {
                description: e.to_string(),
            },
        )
    }

    fn on_acknowledgement_packet_execute(
        &mut self,
        _packet: &Packet,
        _acknowledgement: &Acknowledgement,
        _relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        (ModuleExtras::empty(), Ok(()))
    }

    fn on_timeout_packet_validate(
        &self,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        on_timeout_packet_validate(self, packet, relayer).map_err(|e: TokenTransferError| {
            PacketError::AppModule {
                description: e.to_string(),
            }
        })
    }

    fn on_timeout_packet_execute(
        &mut self,
        packet: &Packet,
        relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        let res = on_timeout_packet_execute(self, packet, relayer);
        (
            res.0,
            res.1
                .map_err(|e: TokenTransferError| PacketError::AppModule {
                    description: e.to_string(),
                }),
        )
    }
}
