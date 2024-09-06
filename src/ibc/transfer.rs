//! ICS-20 Fungible Token Transfer module

use crate::{
    coins::{Address, Amount, Coin, Symbol},
    collections::Map,
    describe::{Builder, Describe},
    encoding::LengthVec,
    orga,
    state::State,
};
use cosmrs::AccountId;
use ed::{Decode, Encode};
use ibc::cosmos_host::utils::cosmos_adr028_escrow_address;
use ibc::{
    apps::transfer::{
        context::{TokenTransferExecutionContext, TokenTransferValidationContext},
        module::*,
        types::{
            error::TokenTransferError, is_receiver_chain_source, packet::PacketData, Memo,
            PrefixedCoin, PrefixedDenom, TracePrefix, VERSION,
        },
    },
    core::{
        channel::types::{
            acknowledgement::Acknowledgement,
            channel::{Counterparty, Order},
            error::{ChannelError, PacketError},
            packet::Packet,
            Version,
        },
        host::types::identifiers::{ChannelId, ConnectionId, PortId},
        router::{
            module::Module,
            types::{event::ModuleEvent, module::ModuleExtras},
        },
    },
    primitives::Signer,
};
const ACCOUNT_PREFIX: &str = "nomic"; // TODO: configurable prefix
impl From<TokenTransferError> for crate::Error {
    fn from(err: TokenTransferError) -> Self {
        crate::Error::Ibc(err.to_string())
    }
}

/// ICS-20 Fungible Token Transfer module
#[orga]
pub struct Transfer {
    /// Maps of account balances for each denom
    pub accounts: Map<Denom, Map<Address, Amount>>,

    #[state(skip)]
    #[serde(skip)]
    incoming_transfer: Option<TransferInfo>,
}

impl std::fmt::Debug for Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transfer").finish()
    }
}

impl Transfer {
    pub(crate) fn incoming_transfer_mut(&mut self) -> &mut Option<TransferInfo> {
        &mut self.incoming_transfer
    }

    /// Returns the balance of an address for the provided [Denom].
    pub fn balance(&self, address: Address, denom: Denom) -> crate::Result<Amount> {
        Ok(*self
            .accounts
            .get(denom)?
            .unwrap_or_default()
            .get(address)?
            .unwrap_or_default())
    }

    /// Returns the balance of an address for the provided [Symbol].
    pub fn symbol_balance<S: Symbol>(&self, address: Address) -> crate::Result<Amount> {
        let denom = S::NAME.try_into()?;

        self.balance(address, denom)
    }

    /// Returns the escrow address for the given port and channel.
    pub fn get_escrow_account(
        &self,
        port_id: &ibc::core::host::types::identifiers::PortId,
        channel_id: &ChannelId,
    ) -> Result<Address, TokenTransferError> {
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
}

impl TokenTransferValidationContext for Transfer {
    type AccountId = Address;

    fn get_port(&self) -> Result<ibc::core::host::types::identifiers::PortId, TokenTransferError> {
        Ok(PortId::transfer())
    }

    fn can_send_coins(&self) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn can_receive_coins(&self) -> Result<(), TokenTransferError> {
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
        _memo: &Memo,
    ) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn escrow_coins_validate(
        &self,
        _from_account: &Self::AccountId,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _coin: &PrefixedCoin,
        _memo: &ibc::apps::transfer::types::Memo,
    ) -> Result<(), TokenTransferError> {
        Ok(())
    }

    fn unescrow_coins_validate(
        &self,
        _to_account: &Self::AccountId,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        _coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        Ok(())
    }
}

type Denom = LengthVec<u8, u8>;

impl TryFrom<PrefixedDenom> for Denom {
    type Error = crate::Error;

    fn try_from(value: PrefixedDenom) -> crate::Result<Self> {
        value.to_string().try_into()
    }
}

impl TryFrom<ibc::apps::transfer::types::Amount> for Amount {
    type Error = crate::Error;

    fn try_from(value: ibc::apps::transfer::types::Amount) -> crate::Result<Self> {
        // TODO: either take dependency on `primitive_types` to get U256, or get
        // try_into<u64> from ibc-rs `amount` type. should not need to use
        // string parsing here.
        let amount = value.to_string();
        let amount = amount.parse::<u64>()?;

        Ok(amount.into())
    }
}

impl From<crate::Error> for TokenTransferError {
    fn from(_err: crate::Error) -> Self {
        TokenTransferError::InvalidToken
    }
}

impl TokenTransferExecutionContext for Transfer {
    fn burn_coins_execute(
        &mut self,
        account: &Self::AccountId,
        coin: &PrefixedCoin,
        _memo: &Memo,
    ) -> Result<(), TokenTransferError> {
        let denom: Denom = coin.denom.clone().try_into()?;
        let amount: Amount = coin.amount.try_into()?;

        let mut denom_balances = self.accounts.entry(denom)?.or_default()?;

        let mut account_balance = denom_balances.entry(*account)?.or_default()?;
        *account_balance = (*account_balance - amount).result()?;

        Ok(())
    }

    fn mint_coins_execute(
        &mut self,
        account: &Self::AccountId,
        coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let denom: Denom = coin.denom.clone().try_into()?;
        let amount: Amount = coin.amount.try_into()?;

        let mut denom_balances = self.accounts.entry(denom)?.or_default()?;

        let mut receiver_balance = denom_balances.entry(*account)?.or_default()?;
        *receiver_balance = (*receiver_balance + amount).result()?;

        Ok(())
    }

    fn escrow_coins_execute(
        &mut self,
        from_account: &Self::AccountId,
        port_id: &PortId,
        channel_id: &ChannelId,
        coin: &PrefixedCoin,
        _memo: &ibc::apps::transfer::types::Memo,
    ) -> Result<(), TokenTransferError> {
        let escrow_address = self.get_escrow_account(port_id, channel_id)?;

        let denom: Denom = coin.denom.clone().try_into()?;
        let mut denom_balances = self.accounts.entry(denom)?.or_default()?;

        let amount: Amount = coin.amount.try_into()?;

        let mut from_balance = denom_balances.entry(*from_account)?.or_default()?;
        *from_balance = (*from_balance - amount).result()?;

        let mut escrow_balance = denom_balances.entry(escrow_address)?.or_default()?;
        *escrow_balance = (*escrow_balance + amount).result()?;

        Ok(())
    }

    fn unescrow_coins_execute(
        &mut self,
        to_account: &Self::AccountId,
        port_id: &PortId,
        channel_id: &ChannelId,
        coin: &PrefixedCoin,
    ) -> Result<(), TokenTransferError> {
        let escrow_address = self.get_escrow_account(port_id, channel_id)?;
        let amount: Amount = coin.amount.try_into()?;

        let denom: Denom = coin.denom.clone().try_into()?;
        let mut denom_balances = self.accounts.entry(denom)?.or_default()?;

        let mut escrow_balance = denom_balances.entry(escrow_address)?.or_default()?;
        *escrow_balance = (*escrow_balance - amount).result()?;

        let mut to_balance = denom_balances.entry(*to_account)?.or_default()?;
        *to_balance = (*to_balance + amount).result()?;

        Ok(())
    }
}

impl<S: Symbol> From<Coin<S>> for PrefixedCoin {
    fn from(value: Coin<S>) -> Self {
        Self {
            amount: value.amount.value.into(),
            denom: S::NAME.parse().unwrap(),
        }
    }
}

impl Module for Transfer {
    fn on_chan_open_init_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
    ) -> Result<Version, ChannelError> {
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
        Ok(Version::new(VERSION.to_string()))
    }

    fn on_chan_open_init_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
    ) -> Result<(ModuleExtras, Version), ChannelError> {
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
        counterparty_version: &Version,
    ) -> Result<Version, ChannelError> {
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
        Ok(Version::new(VERSION.to_string()))
    }

    fn on_chan_open_try_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &Version,
    ) -> Result<(ModuleExtras, Version), ChannelError> {
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
        let (extras, ack) = on_recv_packet_execute(self, packet);

        if let Ok(data) = serde_json::from_slice::<PacketData>(&packet.data) {
            if is_receiver_chain_source(
                packet.port_id_on_a.clone(),
                packet.chan_id_on_a.clone(),
                &data.token.denom,
            ) {
                let incoming_transfer = extras
                    .events
                    .iter()
                    .find(|event| {
                        event.kind == "fungible_token_packet"
                            && event
                                .attributes
                                .contains(&("success".to_string(), "true".to_string()).into())
                    })
                    .map(|event| -> crate::Result<TransferInfo> {
                        let mut denom = data.token.denom.clone();

                        denom.remove_trace_prefix(&TracePrefix::new(
                            packet.port_id_on_a.clone(),
                            packet.chan_id_on_a.clone(),
                        ));

                        let get_attr = |ev: &ModuleEvent, key: &str| {
                            ev.attributes
                                .iter()
                                .find(|attr| attr.key == key)
                                .map(|attr| attr.value.clone())
                                .ok_or_else(|| {
                                    crate::Error::Ibc(format!(
                                        "Missing transfer event attribute {}",
                                        key
                                    ))
                                })
                        };
                        Ok(TransferInfo {
                            denom,
                            amount: get_attr(event, "amount")?.parse()?,
                            memo: get_attr(event, "memo")?,
                            receiver: get_attr(event, "receiver")?,
                            sender: get_attr(event, "sender")?,
                        })
                    })
                    .transpose()
                    .unwrap_or_default();

                if let Some(incoming_transfer) = incoming_transfer {
                    self.incoming_transfer.replace(incoming_transfer);
                }
            }
        };
        (extras, ack)
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

/// Information about an incoming transfer.
#[derive(Debug, Clone)]
pub struct TransferInfo {
    /// The token denom, containing info about its origin.
    pub denom: PrefixedDenom,
    /// The amount of tokens being transferred.
    pub amount: u64,
    /// The sender of the transfer.
    pub sender: String,
    /// The receiver of the transfer.
    pub receiver: String,
    /// ICS-20 memo.
    pub memo: String,
}

impl Describe for TransferInfo {
    fn describe() -> orga::describe::Descriptor {
        Builder::new::<()>().build()
    }
}

impl Encode for TransferInfo {
    fn encode_into<W: std::io::Write>(&self, _dest: &mut W) -> ed::Result<()> {
        unreachable!()
    }
    fn encoding_length(&self) -> ed::Result<usize> {
        unreachable!()
    }
}

impl Decode for TransferInfo {
    fn decode<R: std::io::Read>(_input: R) -> ed::Result<Self> {
        unreachable!()
    }
}

impl State for TransferInfo {
    fn load(_store: orga::store::Store, _bytes: &mut &[u8]) -> orga::Result<Self> {
        unreachable!()
    }

    fn attach(&mut self, _store: orga::store::Store) -> orga::Result<()> {
        unreachable!()
    }

    fn flush<W: std::io::Write>(self, _out: &mut W) -> orga::Result<()> {
        unreachable!()
    }
}

impl crate::encoding::Terminated for TransferInfo {}
