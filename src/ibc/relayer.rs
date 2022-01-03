use bitcoin::hashes::hex::ToHex;
use core::future::Future;
use ibc::core::ics02_client::height::Height;
use ibc::core::ics02_client::misbehaviour::MisbehaviourEvidence;
use ibc::core::ics26_routing::msgs::Ics26Envelope;
use ibc_proto::cosmos::base::abci::v1beta1::TxResponse;
use ibc_relayer::chain::runtime::ChainRuntime;
use ibc_relayer::chain::{ChainEndpoint, HealthCheck};
use ibc_relayer::config::reload::ConfigReload;
use ibc_relayer::error::Error;
use ibc_relayer::foreign_client::ForeignClient;
use ibc_relayer::light_client::{LightBlock, LightClient};
use ibc_relayer::registry::SharedRegistry;
use itertools::Itertools;
use prost_types::Any;
use std::marker::PhantomData;
use tendermint::consensus::Params;
use tendermint_light_client::components;
use tendermint_rpc::endpoint::tx::Response as ResultTx;
use tendermint_rpc::query::{EventType, Query};
use tendermint_rpc::Url;
use tendermint_rpc::{
    endpoint::broadcast::tx_sync::Response, endpoint::status, Client, HttpClient, Order,
};
use tokio::runtime::Runtime as TokioRuntime;

use ibc::clients::ics07_tendermint::client_state::{AllowUpdate, ClientState};
use ibc::clients::ics07_tendermint::consensus_state::ConsensusState as TMConsensusState;
use ibc::clients::ics07_tendermint::header::Header as TmHeader;
use ibc::core::ics02_client::client_consensus::{
    AnyConsensusState, AnyConsensusStateWithHeight, QueryClientEventRequest,
};
use ibc::core::ics02_client::client_state::{AnyClientState, IdentifiedAnyClientState};
use ibc::core::ics02_client::client_type::ClientType;
use ibc::core::ics02_client::events::{self as ClientEvents, UpdateClient};
use ibc::core::ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd};
use ibc::core::ics04_channel::channel::{
    ChannelEnd, IdentifiedChannelEnd, QueryPacketEventDataRequest,
};
use ibc::core::ics04_channel::events as ChannelEvents;
use ibc::core::ics04_channel::packet::{Packet, PacketMsgType, Sequence};
use ibc::core::ics23_commitment::commitment::CommitmentPrefix;
use ibc::core::ics23_commitment::merkle::convert_tm_to_ics_merkle_proof;
use ibc::core::ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId};
use ibc::core::ics24_host::Path::ClientConsensusState as ClientConsensusPath;
use ibc::core::ics24_host::Path::ClientState as ClientStatePath;
use ibc::core::ics24_host::{ClientUpgradePath, Path, IBC_QUERY_PATH, SDK_UPGRADE_QUERY_PATH};
use ibc::events::{from_tx_response_event, IbcEvent};
use ibc::query::{QueryTxHash, QueryTxRequest};
use ibc::signer::Signer;
use ibc::timestamp::Timestamp;
use ibc::Height as ICSHeight;
use ibc::{downcast, query::QueryBlockRequest};
use ibc_proto::cosmos::auth::v1beta1::{BaseAccount, EthAccount, QueryAccountRequest};
use ibc_proto::cosmos::base::tendermint::v1beta1::service_client::ServiceClient;
use ibc_proto::cosmos::base::tendermint::v1beta1::GetNodeInfoRequest;
use ibc_proto::cosmos::base::v1beta1::Coin;
use ibc_proto::cosmos::tx::v1beta1::mode_info::{Single, Sum};
use ibc_proto::cosmos::tx::v1beta1::{
    AuthInfo, Fee, ModeInfo, SignDoc, SignerInfo, SimulateRequest, SimulateResponse, Tx, TxBody,
    TxRaw,
};

use bip39::{Language, Mnemonic, MnemonicType, Seed};
use chrono::DateTime;
use ibc_proto::ibc::core::channel::v1::{
    PacketState, QueryChannelClientStateRequest, QueryChannelsRequest,
    QueryConnectionChannelsRequest, QueryNextSequenceReceiveRequest,
    QueryPacketAcknowledgementsRequest, QueryPacketCommitmentsRequest, QueryUnreceivedAcksRequest,
    QueryUnreceivedPacketsRequest,
};
use ibc_proto::ibc::core::client::v1::{QueryClientStatesRequest, QueryConsensusStatesRequest};
use ibc_proto::ibc::core::commitment::v1::MerkleProof;
use ibc_proto::ibc::core::connection::v1::{
    QueryClientConnectionsRequest, QueryConnectionsRequest,
};
use ibc_relayer::chain::handle::{ChainHandlePair, ProdChainHandle};
use ibc_relayer::config::types::Memo;
use ibc_relayer::config::Config;
use ibc_relayer::event::monitor::{EventMonitor, EventReceiver};
use ibc_relayer::keyring::{KeyEntry, KeyRing, Store};
use ibc_relayer::light_client::tendermint::LightClient as TmLightClient;
use ibc_relayer::light_client::Verified;
use ibc_relayer::rest;
use ibc_relayer::supervisor::Supervisor;
use ibc_relayer::{chain::QueryResponse, chain::StatusResponse, event::monitor::TxMonitorCmd};
use ibc_relayer::{
    config::{AddressType, ChainConfig, GasPrice},
    sdk_error::sdk_error_from_tx_sync_error_code,
};

use tendermint_light_client::types::{LightBlock as TMLightBlock, PeerId, TrustThreshold};

use std::convert::{TryFrom, TryInto};
use std::sync::{Arc, RwLock};
use std::time::Duration;

fn spawn_rest_server(config: &Arc<RwLock<Config>>) -> Option<rest::Receiver> {
    let rest = config.read().expect("poisoned lock").rest.clone();

    if rest.enabled {
        let rest_config = ibc_relayer_rest::Config::new(rest.host, rest.port);
        let (_, rest_receiver) = ibc_relayer_rest::server::spawn(rest_config);
        Some(rest_receiver)
    } else {
        panic!("[rest] address not configured, REST server disabled");
        None
    }
}

use super::Ibc;
use crate::call::Call;
use crate::client::{AsyncCall, AsyncQuery, Client as OrgaClient};

pub trait GetIbcClient: Sync + Send + 'static {
    type Parent: IbcClientParent;
    fn get_ibc_client() -> <Ibc as OrgaClient<Self::Parent>>::Client;
}

type IbcClient<T> = <Ibc as OrgaClient<T>>::Client;
pub trait IbcClientParent = Send + 'static + Clone + AsyncCall<Call = <Ibc as Call>::Call>;
pub fn run_relayer<T: GetIbcClient>() {
    println!("Starting relayer...");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let cfg = ChainConfig {
        id: ChainId::new("my-chain1".into(), 1),
        rpc_addr: "http://localhost:26657".parse().unwrap(),
        websocket_addr: "ws://localhost:26657/websocket".parse().unwrap(),
        grpc_addr: "http://127.0.0.1:42069".parse().unwrap(),
        rpc_timeout: Duration::from_secs(10),
        account_prefix: "cosmos".into(),
        key_name: "test-key".into(),
        key_store_type: Store::Test,
        store_prefix: "ibc".into(),
        default_gas: None,
        max_gas: None,
        gas_adjustment: None,
        max_msg_num: Default::default(),
        max_tx_size: Default::default(),
        clock_drift: Duration::from_secs(5),
        max_block_time: Duration::from_secs(60),
        trusting_period: Some(Duration::from_secs(60 * 60 * 24 * 7 * 2)),
        memo_prefix: Default::default(),
        trust_threshold: TrustThreshold::ONE_THIRD,
        gas_price: GasPrice::new(0.0, "stake".into()),
        packet_filter: Default::default(),
        address_type: Default::default(),
    };
    let mut cfg2 = cfg.clone();
    cfg2.id = ChainId::new("my-chain2".into(), 1);
    cfg2.rpc_addr = "http://localhost:26667".parse().unwrap();
    cfg2.websocket_addr = "ws://localhost:26667/websocket".parse().unwrap();

    let arc_rt = Arc::new(rt);
    let handle: ProdChainHandle =
        ChainRuntime::<OrgaChainEndpoint<T>>::spawn(cfg.clone(), arc_rt.clone()).unwrap();
    let handle2: ProdChainHandle =
        ChainRuntime::<OrgaChainEndpoint<T>>::spawn(cfg2.clone(), arc_rt).unwrap();
    println!("got chain handles");
    let mut config = Config::default();
    config.chains.push(cfg);
    config.chains.push(cfg2);
    config.rest.enabled = true;

    let config = Arc::new(RwLock::new(config));
    let rest_rx = spawn_rest_server(&config).unwrap();
    let (mut supervisor, tx_send) =
        Supervisor::<ProdChainHandle>::new(config.clone(), Some(rest_rx));
    // let cfg_reload = ConfigReload::new("./config2.toml", config, tx_send);
    std::thread::spawn(move || {
        supervisor.run().unwrap();
    });

    let client = ForeignClient::<ProdChainHandle, ProdChainHandle>::restore(
        Default::default(),
        handle,
        handle2,
    );

    let res = client.build_create_client_and_send();
    println!("{:?}", res);

    println!("Supervisor running in background");
}
pub struct OrgaChainEndpoint<T> {
    config: ChainConfig,
    rt: Arc<TokioRuntime>,
    rpc_client: HttpClient,
    keybase: KeyRing,
    _marker: PhantomData<T>,
}

impl<T> OrgaChainEndpoint<T>
where
    T: GetIbcClient,
{
    /// Performs validation of chain-specific configuration
    /// parameters against the chain's genesis configuration.
    ///
    /// Currently, validates the following:
    ///     - the configured `max_tx_size` is appropriate
    ///     - the trusting period is greater than zero
    ///     - the trusting period is smaller than the unbonding period
    ///     - the default gas is smaller than the max gas
    ///
    /// Emits a log warning in case any error is encountered and
    /// exits early without doing subsequent validations.
    pub fn validate_params(&self) -> Result<(), Error> {
        todo!()
    }

    /// The unbonding period of this chain
    pub fn unbonding_period(&self) -> Result<Duration, Error> {
        Ok(Duration::from_secs(1209600 * 2))
    }

    fn rpc_client(&self) -> &HttpClient {
        &self.rpc_client
    }

    pub fn config(&self) -> &ChainConfig {
        &self.config
    }

    /// Query the consensus parameters via an RPC query
    /// Specific to the SDK and used only for Tendermint client create
    pub fn query_consensus_params(&self) -> Result<Params, Error> {
        Ok(self
            .block_on(self.rpc_client().genesis())
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?
            .consensus_params)
    }

    /// Run a future to completion on the Tokio runtime.
    fn block_on<F: Future>(&self, f: F) -> F::Output {
        self.rt.block_on(f)
    }

    fn send_tx(&mut self, proto_msgs: Vec<Any>) -> Result<Response, Error> {
        todo!()
    }

    /// Try to simulate the given tx in order to estimate how much gas will be needed to submit it.
    ///
    /// It is possible that a batch of messages are fragmented by the caller (`send_msgs`) such that
    /// they do not individually verify. For example for the following batch:
    /// [`MsgUpdateClient`, `MsgRecvPacket`, ..., `MsgRecvPacket`]
    ///
    /// If the batch is split in two TX-es, the second one will fail the simulation in `deliverTx` check.
    /// In this case we use the `default_gas` param.
    fn estimate_gas(&self, tx: Tx) -> Result<u64, Error> {
        todo!()
    }

    /// The default amount of gas the relayer is willing to pay for a transaction,
    /// when it cannot simulate the tx and therefore estimate the gas amount needed.
    fn default_gas(&self) -> u64 {
        self.config.default_gas.unwrap_or_else(|| self.max_gas())
    }

    /// The maximum amount of gas the relayer is willing to pay for a transaction
    fn max_gas(&self) -> u64 {
        self.config.max_gas.unwrap_or(1000000)
    }

    /// The gas price
    fn gas_price(&self) -> &GasPrice {
        &self.config.gas_price
    }

    /// The gas price adjustment
    fn gas_adjustment(&self) -> f64 {
        self.config.gas_adjustment.unwrap_or(0.0)
    }

    /// Adjusts the fee based on the configured `gas_adjustment` to prevent out of gas errors.
    /// The actual gas cost, when a transaction is executed, may be slightly higher than the
    /// one returned by the simulation.
    fn apply_adjustment_to_gas(&self, gas_amount: u64) -> u64 {
        todo!()
    }

    /// The maximum fee the relayer pays for a transaction
    fn max_fee_in_coins(&self) -> Coin {
        todo!()
    }

    /// The fee in coins based on gas amount
    fn fee_from_gas_in_coins(&self, gas: u64) -> Coin {
        todo!()
    }

    /// The maximum number of messages included in a transaction
    fn max_msg_num(&self) -> usize {
        self.config.max_msg_num.into()
    }

    /// The maximum size of any transaction sent by the relayer to this chain
    fn max_tx_size(&self) -> usize {
        self.config.max_tx_size.into()
    }

    fn query(&self, data: Path, height: ICSHeight, prove: bool) -> Result<QueryResponse, Error> {
        todo!()
    }

    /// Perform an ABCI query against the client upgrade sub-store.
    /// Fetches both the target data, as well as the proof.
    ///
    /// The data is returned in its raw format `Vec<u8>`, and is either
    /// the client state (if the target path is [`UpgradedClientState`]),
    /// or the client consensus state ([`UpgradedClientConsensusState`]).
    fn query_client_upgrade_state(
        &self,
        data: ClientUpgradePath,
        height: Height,
    ) -> Result<(Vec<u8>, MerkleProof), Error> {
        todo!()
    }

    fn key(&self) -> Result<KeyEntry, Error> {
        self.keybase()
            .get_key(&self.config.key_name)
            .map_err(Error::key_base)
    }

    fn key_bytes(&self, key: &KeyEntry) -> Result<Vec<u8>, Error> {
        let mut pk_buf = Vec::new();
        prost::Message::encode(&key.public_key.public_key.to_bytes(), &mut pk_buf)
            .map_err(|e| Error::protobuf_encode(String::from("Key bytes"), e))?;
        Ok(pk_buf)
    }

    fn key_and_bytes(&self) -> Result<(KeyEntry, Vec<u8>), Error> {
        let key = self.key()?;
        let key_bytes = self.key_bytes(&key)?;
        Ok((key, key_bytes))
    }

    fn signer(&self, sequence: u64) -> Result<SignerInfo, Error> {
        let (_key, pk_buf) = self.key_and_bytes()?;
        let pk_type = match &self.config.address_type {
            AddressType::Cosmos => "/cosmos.crypto.secp256k1.PubKey".to_string(),
            AddressType::Ethermint { pk_type } => pk_type.clone(),
        };
        // Create a MsgSend proto Any message
        let pk_any = Any {
            type_url: pk_type,
            value: pk_buf,
        };

        let single = Single { mode: 1 };
        let sum_single = Some(Sum::Single(single));
        let mode = Some(ModeInfo { sum: sum_single });
        let signer_info = SignerInfo {
            public_key: Some(pk_any),
            mode_info: mode,
            sequence,
        };
        Ok(signer_info)
    }

    fn max_fee(&self) -> Fee {
        Fee {
            amount: vec![self.max_fee_in_coins()],
            gas_limit: self.max_gas(),
            payer: "".to_string(),
            granter: "".to_string(),
        }
    }

    fn fee_with_gas(&self, gas_limit: u64) -> Fee {
        let adjusted_gas_limit = self.apply_adjustment_to_gas(gas_limit);

        Fee {
            amount: vec![self.fee_from_gas_in_coins(adjusted_gas_limit)],
            gas_limit: adjusted_gas_limit,
            payer: "".to_string(),
            granter: "".to_string(),
        }
    }

    fn signed_doc(
        &self,
        body_bytes: Vec<u8>,
        auth_info_bytes: Vec<u8>,
        account_number: u64,
    ) -> Result<Vec<u8>, Error> {
        let sign_doc = SignDoc {
            body_bytes,
            auth_info_bytes,
            chain_id: self.config.clone().id.to_string(),
            account_number,
        };

        // A protobuf serialization of a SignDoc
        let mut signdoc_buf = Vec::new();
        prost::Message::encode(&sign_doc, &mut signdoc_buf)
            .map_err(|e| Error::protobuf_encode(String::from("SignDoc"), e))?;

        // Sign doc
        let signed = self
            .keybase
            .sign_msg(
                &self.config.key_name,
                signdoc_buf,
                &self.config.address_type,
            )
            .map_err(Error::key_base)?;

        Ok(signed)
    }

    /// Given a vector of `TxSyncResult` elements,
    /// each including a transaction response hash for one or more messages, periodically queries the chain
    /// with the transaction hashes to get the list of IbcEvents included in those transactions.
    pub fn wait_for_block_commits(
        &self,
        mut tx_sync_results: Vec<TxSyncResult>,
    ) -> Result<Vec<TxSyncResult>, Error> {
        todo!()
    }

    fn trusting_period(&self, unbonding_period: Duration) -> Duration {
        self.config
            .trusting_period
            .unwrap_or(2 * unbonding_period / 3)
    }

    /// Returns the preconfigured memo to be used for every submitted transaction
    fn tx_memo(&self) -> &Memo {
        &self.config.memo_prefix
    }

    /// Query the chain status via an RPC query
    fn status(&self) -> Result<status::Response, Error> {
        let status = self
            .block_on(self.rpc_client().status())
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

        if status.sync_info.catching_up {
            return Err(Error::chain_not_caught_up(
                self.config.rpc_addr.to_string(),
                self.config().id.clone(),
            ));
        }

        Ok(status)
    }

    /// Query the chain's latest height
    pub fn query_latest_height(&self) -> Result<ICSHeight, Error> {
        todo!()
        // let status = self.status()?;
        // Ok(ICSHeight {
        //     revision_number: ChainId::chain_version(status.node_info.network.as_str()),
        //     revision_height: u64::from(status.sync_info.latest_block_height),
        // })
    }
}

impl<T> ChainEndpoint for OrgaChainEndpoint<T>
where
    T: GetIbcClient,
{
    type LightBlock = TMLightBlock;
    type Header = TmHeader;
    type ConsensusState = TMConsensusState;
    type ClientState = ClientState;
    type LightClient = OrgaLightClient<T>;

    fn bootstrap(config: ChainConfig, rt: Arc<TokioRuntime>) -> Result<Self, Error> {
        let rpc_client = HttpClient::new(config.rpc_addr.clone())
            .map_err(|e| Error::rpc(config.rpc_addr.clone(), e))?;

        let mut keybase = KeyRing::new(config.key_store_type, &config.account_prefix, &config.id)
            .map_err(Error::key_base)?;

        let words = Mnemonic::new(MnemonicType::Words12, Language::English).into_phrase();
        let key = keybase
            .key_from_mnemonic(&words, &Default::default(), &Default::default())
            .unwrap();

        keybase.add_key(&config.key_name, key).unwrap();

        let chain = Self {
            config,
            rpc_client,
            rt,
            keybase,
            _marker: Default::default(),
        };

        Ok(chain)
    }

    fn init_light_client(&self) -> Result<Self::LightClient, Error> {
        let peer_id: PeerId = self
            .rt
            .block_on(self.rpc_client.status())
            .map(|s| s.node_info.id)
            .map_err(|e| Error::rpc(self.config.rpc_addr.clone(), e))?;

        let light_client = OrgaLightClient::from_config(&self.config, peer_id)?;

        Ok(light_client)
    }

    fn init_event_monitor(
        &self,
        rt: Arc<TokioRuntime>,
    ) -> Result<(EventReceiver, TxMonitorCmd), Error> {
        let (mut event_monitor, event_receiver, monitor_tx) = EventMonitor::new(
            self.config.id.clone(),
            self.config.websocket_addr.clone(),
            rt,
        )
        .map_err(Error::event_monitor)?;

        event_monitor.subscribe().map_err(Error::event_monitor)?;

        std::thread::spawn(move || event_monitor.run());

        Ok((event_receiver, monitor_tx))
    }

    fn id(&self) -> &ChainId {
        &self.config.id
    }

    fn shutdown(self) -> Result<(), Error> {
        Ok(())
    }
    fn health_check(&self) -> Result<HealthCheck, Error> {
        todo!()
    }
    fn keybase(&self) -> &KeyRing {
        &self.keybase
    }
    fn keybase_mut(&mut self) -> &mut KeyRing {
        &mut self.keybase
    }

    fn send_messages_and_wait_commit(
        &mut self,
        proto_msgs: Vec<Any>,
    ) -> Result<Vec<IbcEvent>, Error> {
        if proto_msgs.is_empty() {
            return Ok(vec![]);
        }
        let mut tx_sync_results = vec![];

        let mut n = 0;
        let mut size = 0;
        let mut msg_batch = vec![];
        for msg in proto_msgs.iter() {
            let envelope: Ics26Envelope = msg.clone().try_into().unwrap();
            println!("{:#?}", envelope);
            msg_batch.push(msg.clone());
            let mut buf = Vec::new();
            prost::Message::encode(msg, &mut buf)
                .map_err(|e| Error::protobuf_encode(String::from("Message"), e))?;
            n += 1;
            size += buf.len();
            if n >= self.max_msg_num() || size >= self.max_tx_size() {
                let events_per_tx = vec![IbcEvent::default(); msg_batch.len()];
                let tx_sync_result = self.send_tx(msg_batch)?;
                tx_sync_results.push(TxSyncResult {
                    response: tx_sync_result,
                    events: events_per_tx,
                });
                n = 0;
                size = 0;
                msg_batch = vec![];
            }
        }

        self.rt.block_on(async {
            let mut client = T::get_ibc_client();
        });
        todo!()
    }
    fn send_messages_and_wait_check_tx(
        &mut self,
        proto_msgs: Vec<Any>,
    ) -> Result<Vec<tendermint_rpc::endpoint::broadcast::tx_sync::Response>, Error> {
        todo!()
    }
    fn get_signer(&mut self) -> Result<Signer, Error> {
        // Get the key from key seed file
        let key = self
            .keybase()
            .get_key(&self.config.key_name)
            .map_err(|e| Error::key_not_found(self.config.key_name.clone(), e))?;

        let bech32 = encode_to_bech32(&key.address.to_hex(), &self.config.account_prefix)?;
        Ok(Signer::new(bech32))
    }
    fn config(&self) -> ChainConfig {
        self.config.clone()
    }
    fn get_key(&mut self) -> Result<KeyEntry, Error> {
        todo!()
    }
    fn add_key(&mut self, key_name: &str, key: KeyEntry) -> Result<(), Error> {
        todo!()
    }
    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        todo!()
    }
    fn query_status(&self) -> Result<StatusResponse, Error> {
        let status = self.status()?;

        let time = DateTime::from(status.sync_info.latest_block_time);
        let height = ICSHeight {
            revision_number: ChainId::chain_version(status.node_info.network.as_str()),
            revision_height: u64::from(status.sync_info.latest_block_height),
        };

        Ok(StatusResponse {
            height,
            timestamp: Timestamp::from_datetime(time),
        })
    }
    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        todo!()
    }
    fn query_client_state(
        &self,
        client_id: &ClientId,
        height: ICSHeight,
    ) -> Result<Self::ClientState, Error> {
        todo!()
    }
    fn query_consensus_states(
        &self,
        request: QueryConsensusStatesRequest,
    ) -> Result<Vec<AnyConsensusStateWithHeight>, Error> {
        todo!()
    }
    fn query_consensus_state(
        &self,
        client_id: ClientId,
        consensus_height: ICSHeight,
        query_height: ICSHeight,
    ) -> Result<AnyConsensusState, Error> {
        todo!()
    }
    fn query_upgraded_client_state(
        &self,
        height: ICSHeight,
    ) -> Result<(Self::ClientState, MerkleProof), Error> {
        todo!()
    }
    fn query_upgraded_consensus_state(
        &self,
        height: ICSHeight,
    ) -> Result<(Self::ConsensusState, MerkleProof), Error> {
        todo!()
    }
    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        todo!()
    }
    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        todo!()
    }
    fn query_connection(
        &self,
        connection_id: &ConnectionId,
        height: ICSHeight,
    ) -> Result<ConnectionEnd, Error> {
        todo!()
    }
    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        todo!()
    }
    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        todo!()
    }
    fn query_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: ICSHeight,
    ) -> Result<ChannelEnd, Error> {
        todo!()
    }
    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        todo!()
    }
    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<PacketState>, ICSHeight), Error> {
        todo!()
    }
    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<u64>, Error> {
        todo!()
    }
    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<PacketState>, ICSHeight), Error> {
        todo!()
    }
    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<u64>, Error> {
        todo!()
    }
    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
    ) -> Result<Sequence, Error> {
        todo!()
    }
    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEvent>, Error> {
        todo!()
    }
    fn query_blocks(
        &self,
        request: QueryBlockRequest,
    ) -> Result<(Vec<IbcEvent>, Vec<IbcEvent>), Error> {
        todo!()
    }
    fn proven_client_state(
        &self,
        client_id: &ClientId,
        height: ICSHeight,
    ) -> Result<(Self::ClientState, MerkleProof), Error> {
        todo!()
    }
    fn proven_connection(
        &self,
        connection_id: &ConnectionId,
        height: ICSHeight,
    ) -> Result<(ConnectionEnd, MerkleProof), Error> {
        todo!()
    }
    fn proven_client_consensus(
        &self,
        client_id: &ClientId,
        consensus_height: ICSHeight,
        height: ICSHeight,
    ) -> Result<(Self::ConsensusState, MerkleProof), Error> {
        todo!()
    }
    fn proven_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: ICSHeight,
    ) -> Result<(ChannelEnd, MerkleProof), Error> {
        todo!()
    }
    fn proven_packet(
        &self,
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: ICSHeight,
    ) -> Result<(Vec<u8>, MerkleProof), Error> {
        todo!()
    }
    fn build_client_state(
        &self,
        height: ICSHeight,
        dst_config: ChainConfig,
    ) -> Result<Self::ClientState, Error> {
        let unbonding_period = self.unbonding_period()?;

        let max_clock_drift = calculate_client_state_drift(self.config(), &dst_config);

        // Build the client state.
        ClientState::new(
            self.id().clone(),
            self.config.trust_threshold.into(),
            self.trusting_period(unbonding_period),
            unbonding_period,
            max_clock_drift,
            height,
            ICSHeight::zero(),
            vec!["upgrade".to_string(), "upgradedIBCState".to_string()],
            AllowUpdate {
                after_expiry: true,
                after_misbehaviour: true,
            },
        )
        .map_err(Error::ics07)
    }
    fn build_consensus_state(
        &self,
        light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        Ok(TMConsensusState::from(light_block.signed_header.header))
    }
    fn build_header(
        &self,
        trusted_height: ICSHeight,
        target_height: ICSHeight,
        client_state: &AnyClientState,
        light_client: &mut Self::LightClient,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        todo!()
    }
}

pub struct OrgaLightClient<T> {
    chain_id: ChainId,
    peer_id: PeerId,
    io: components::io::ProdIo,
    _marker: PhantomData<T>,
}

impl<T> LightClient<OrgaChainEndpoint<T>> for OrgaLightClient<T>
where
    T: GetIbcClient,
{
    fn header_and_minimal_set(
        &mut self,
        trusted: Height,
        target: Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<<OrgaChainEndpoint<T> as ChainEndpoint>::Header>, Error> {
        let verified = self.verify(trusted, target, client_state)?;
        let target = verified.target;
        let supporting = verified.supporting;
        // let Verified { target, supporting } = self.verify(trusted, target, client_state)?;
        let (target, supporting) = self.adjust_headers(trusted, target, supporting)?;
        Ok(Verified { target, supporting })
    }

    fn verify(
        &mut self,
        trusted: Height,
        target: Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<<OrgaChainEndpoint<T> as ChainEndpoint>::LightBlock>, Error> {
        let target_height =
            TMHeight::try_from(target.revision_height).map_err(Error::invalid_height)?;

        let client = self.prepare_client(client_state)?;
        let mut state = self.prepare_state(trusted)?;

        // Verify the target header
        let target = client
            .verify_to_target(target_height, &mut state)
            .map_err(|e| Error::light_client(self.chain_id.to_string(), e))?;

        // Collect the verification trace for the target block
        let target_trace = state.get_trace(target.height());

        // Compute the minimal supporting set, sorted by ascending height
        let supporting = target_trace
            .into_iter()
            .filter(|lb| lb.height() != target.height())
            .unique_by(TmLightBlock::height)
            .sorted_by_key(TmLightBlock::height)
            .collect_vec();

        Ok(Verified { target, supporting })
    }
    fn check_misbehaviour(
        &mut self,
        update: UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        todo!()
    }
    fn fetch(
        &mut self,
        height: Height,
    ) -> Result<<OrgaChainEndpoint<T> as ChainEndpoint>::LightBlock, Error> {
        todo!()
    }
}

use tendermint_light_client::{
    components::io::AtHeight,
    light_client::{LightClient as TMLightClient, Options as TmOptions},
    operations,
    state::State as LightClientState,
    store::{memory::MemoryStore, LightStore},
    types::Height as TMHeight,
    types::LightBlock as TmLightBlock,
    types::Status,
};
use tendermint_rpc as rpc;

impl<T> OrgaLightClient<T>
where
    T: GetIbcClient,
{
    pub fn from_config(config: &ChainConfig, peer_id: PeerId) -> Result<Self, Error> {
        let rpc_client = HttpClient::new(config.rpc_addr.clone())
            .map_err(|e| Error::rpc(config.rpc_addr.clone(), e))?;

        let io = components::io::ProdIo::new(peer_id, rpc_client, Some(config.rpc_timeout));

        Ok(Self {
            chain_id: config.id.clone(),
            peer_id,
            io,
            _marker: Default::default(),
        })
    }

    fn prepare_client(&self, client_state: &AnyClientState) -> Result<TMLightClient, Error> {
        let clock = components::clock::SystemClock;
        let hasher = operations::hasher::ProdHasher;
        let verifier = components::verifier::ProdVerifier::default();
        let scheduler = components::scheduler::basic_bisecting_schedule;

        let client_state =
            downcast!(client_state => AnyClientState::Tendermint).ok_or_else(|| {
                Error::client_type_mismatch(ClientType::Tendermint, client_state.client_type())
            })?;

        let params = TmOptions {
            trust_threshold: client_state
                .trust_level
                .try_into()
                .map_err(Error::light_client_state)?,
            trusting_period: client_state.trusting_period,
            clock_drift: client_state.max_clock_drift,
        };

        Ok(TMLightClient::new(
            self.peer_id,
            params,
            clock,
            scheduler,
            verifier,
            hasher,
            self.io.clone(),
        ))
    }

    fn prepare_state(&self, trusted: ibc::Height) -> Result<LightClientState, Error> {
        let trusted_height =
            TMHeight::try_from(trusted.revision_height).map_err(Error::invalid_height)?;

        let trusted_block = self.fetch_light_block(AtHeight::At(trusted_height))?;

        let mut store = MemoryStore::new();
        store.insert(trusted_block, Status::Trusted);

        Ok(LightClientState::new(store))
    }

    fn fetch_light_block(&self, height: AtHeight) -> Result<TmLightBlock, Error> {
        use tendermint_light_client::components::io::Io;
        self.io
            .fetch_light_block(height)
            .map_err(|e| Error::light_client_io(self.chain_id.to_string(), e))
    }

    fn adjust_headers(
        &mut self,
        trusted_height: ibc::Height,
        target: TmLightBlock,
        supporting: Vec<TmLightBlock>,
    ) -> Result<(TmHeader, Vec<TmHeader>), Error> {
        // Get the light block at trusted_height + 1 from chain.
        //
        // NOTE: This is needed to get the next validator set. While there is a next validator set
        //       in the light block at trusted height, the proposer is not
        //       known/set in this set.
        println!("called adjust_headers");
        let trusted_validator_set = self.fetch(trusted_height.increment())?.validators;

        let mut supporting_headers = Vec::with_capacity(supporting.len());

        let mut current_trusted_height = trusted_height;
        let mut current_trusted_validators = trusted_validator_set.clone();

        for support in supporting {
            let header = TmHeader {
                signed_header: support.signed_header.clone(),
                validator_set: support.validators,
                trusted_height: current_trusted_height,
                trusted_validator_set: current_trusted_validators,
            };

            // This header is now considered to be the currently trusted header
            current_trusted_height = header.height();

            // Therefore we can now trust the next validator set, see NOTE above.
            current_trusted_validators = self.fetch(header.height().increment())?.validators;

            supporting_headers.push(header);
        }

        // a) Set the trusted height of the target header to the height of the previous
        // supporting header if any, or to the initial trusting height otherwise.
        //
        // b) Set the trusted validators of the target header to the validators of the successor to
        // the last supporting header if any, or to the initial trusted validators otherwise.
        let (latest_trusted_height, latest_trusted_validator_set) = match supporting_headers.last()
        {
            Some(prev_header) => {
                let prev_succ = self.fetch(prev_header.height().increment())?;
                (prev_header.height(), prev_succ.validators)
            }
            None => (trusted_height, trusted_validator_set),
        };

        let target_header = TmHeader {
            signed_header: target.signed_header,
            validator_set: target.validators,
            trusted_height: latest_trusted_height,
            trusted_validator_set: latest_trusted_validator_set,
        };

        Ok((target_header, supporting_headers))
    }
}

use bech32::{ToBase32, Variant};
use std::str::FromStr;
use tendermint::account::Id as AccountId;
fn encode_to_bech32(address: &str, account_prefix: &str) -> Result<String, Error> {
    let account = AccountId::from_str(address).map_err(|_| Error::tx_no_confirmation())?; // TODO: use correct error variant

    let encoded = bech32::encode(account_prefix, account.to_base32(), Variant::Bech32)
        .map_err(Error::bech32_encoding)?;

    Ok(encoded)
}

pub struct TxSyncResult {
    // the broadcast_tx_sync response
    response: Response,
    // the events generated by a Tx once executed
    events: Vec<IbcEvent>,
}

/// Compute the `max_clock_drift` for a (new) client state
/// as a function of the configuration of the source chain
/// and the destination chain configuration.
///
/// The client state clock drift must account for destination
/// chain block frequency and clock drift on source and dest.
/// https://github.com/informalsystems/ibc-rs/issues/1445
fn calculate_client_state_drift(
    src_chain_config: &ChainConfig,
    dst_chain_config: &ChainConfig,
) -> Duration {
    src_chain_config.clock_drift + dst_chain_config.clock_drift + dst_chain_config.max_block_time
}
