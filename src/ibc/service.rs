use std::marker::PhantomData;
use std::str::FromStr;

use ibc::core::ics24_host::identifier::{ClientId, PortId};
use ibc::{
    clients::ics07_tendermint::{
        client_state::ClientState as TmClientState,
        consensus_state::ConsensusState as TmConsensusState,
    },
    core::{
        ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd},
        ics04_channel::{
            channel::{ChannelEnd, IdentifiedChannelEnd},
            commitment::{AcknowledgementCommitment, PacketCommitment},
            packet::Sequence,
        },
        ics24_host::{
            identifier::{ChannelId, ConnectionId},
            path::{
                AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath,
                ClientStatePath, CommitmentPath, ConnectionPath, ReceiptPath,
            },
        },
    },
};

use ibc_proto::cosmos::auth::v1beta1::{
    query_server::Query as AuthQuery, query_server::QueryServer as AuthQueryServer,
    AddressBytesToStringRequest, AddressBytesToStringResponse, AddressStringToBytesRequest,
    AddressStringToBytesResponse, Bech32PrefixRequest, Bech32PrefixResponse,
    QueryAccountAddressByIdRequest, QueryAccountAddressByIdResponse, QueryAccountRequest,
    QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
    QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse, QueryModuleAccountsRequest,
    QueryModuleAccountsResponse, QueryParamsRequest as AuthQueryParamsRequest,
    QueryParamsResponse as AuthQueryParamsResponse,
};
use ibc_proto::cosmos::{
    bank::v1beta1::{
        query_server::{Query as BankQuery, QueryServer as BankQueryServer},
        QueryAllBalancesRequest, QueryAllBalancesResponse, QueryBalanceRequest,
        QueryBalanceResponse, QueryDenomMetadataRequest, QueryDenomMetadataResponse,
        QueryDenomOwnersRequest, QueryDenomOwnersResponse, QueryDenomsMetadataRequest,
        QueryDenomsMetadataResponse, QueryParamsRequest, QueryParamsResponse,
        QuerySpendableBalancesRequest, QuerySpendableBalancesResponse, QuerySupplyOfRequest,
        QuerySupplyOfResponse, QueryTotalSupplyRequest, QueryTotalSupplyResponse,
    },
    base::v1beta1::Coin as RawCoin,
};
use ibc_proto::ibc::core::{
    channel::v1::{
        query_server::{Query as ChannelQuery, QueryServer as ChannelQueryServer},
        Channel as RawChannelEnd, IdentifiedChannel as RawIdentifiedChannel, PacketState,
        QueryChannelClientStateRequest, QueryChannelClientStateResponse,
        QueryChannelConsensusStateRequest, QueryChannelConsensusStateResponse, QueryChannelRequest,
        QueryChannelResponse, QueryChannelsRequest, QueryChannelsResponse,
        QueryConnectionChannelsRequest, QueryConnectionChannelsResponse,
        QueryNextSequenceReceiveRequest, QueryNextSequenceReceiveResponse,
        QueryPacketAcknowledgementRequest, QueryPacketAcknowledgementResponse,
        QueryPacketAcknowledgementsRequest, QueryPacketAcknowledgementsResponse,
        QueryPacketCommitmentRequest, QueryPacketCommitmentResponse, QueryPacketCommitmentsRequest,
        QueryPacketCommitmentsResponse, QueryPacketReceiptRequest, QueryPacketReceiptResponse,
        QueryUnreceivedAcksRequest, QueryUnreceivedAcksResponse, QueryUnreceivedPacketsRequest,
        QueryUnreceivedPacketsResponse,
    },
    client::v1::{
        query_server::{Query as ClientQuery, QueryServer as ClientQueryServer},
        ConsensusStateWithHeight, Height as RawHeight, IdentifiedClientState,
        QueryClientParamsRequest, QueryClientParamsResponse, QueryClientStateRequest,
        QueryClientStateResponse, QueryClientStatesRequest, QueryClientStatesResponse,
        QueryClientStatusRequest, QueryClientStatusResponse, QueryConsensusStateHeightsRequest,
        QueryConsensusStateHeightsResponse, QueryConsensusStateRequest,
        QueryConsensusStateResponse, QueryConsensusStatesRequest, QueryConsensusStatesResponse,
        QueryUpgradedClientStateRequest, QueryUpgradedClientStateResponse,
        QueryUpgradedConsensusStateRequest, QueryUpgradedConsensusStateResponse,
    },
    connection::v1::{
        query_server::{Query as ConnectionQuery, QueryServer as ConnectionQueryServer},
        ConnectionEnd as RawConnectionEnd, IdentifiedConnection as RawIdentifiedConnection,
        QueryClientConnectionsRequest, QueryClientConnectionsResponse,
        QueryConnectionClientStateRequest, QueryConnectionClientStateResponse,
        QueryConnectionConsensusStateRequest, QueryConnectionConsensusStateResponse,
        QueryConnectionRequest, QueryConnectionResponse, QueryConnectionsRequest,
        QueryConnectionsResponse,
    },
};
use ibc_proto::{
    cosmos::staking::v1beta1::{
        query_server::{Query as StakingQuery, QueryServer as StakingQueryServer},
        Params, QueryDelegationRequest, QueryDelegationResponse, QueryDelegatorDelegationsRequest,
        QueryDelegatorDelegationsResponse, QueryDelegatorUnbondingDelegationsRequest,
        QueryDelegatorUnbondingDelegationsResponse, QueryDelegatorValidatorRequest,
        QueryDelegatorValidatorResponse, QueryDelegatorValidatorsRequest,
        QueryDelegatorValidatorsResponse, QueryHistoricalInfoRequest, QueryHistoricalInfoResponse,
        QueryParamsRequest as StakingQueryParamsRequest,
        QueryParamsResponse as StakingQueryParamsResponse, QueryPoolRequest, QueryPoolResponse,
        QueryRedelegationsRequest, QueryRedelegationsResponse, QueryUnbondingDelegationRequest,
        QueryUnbondingDelegationResponse, QueryValidatorDelegationsRequest,
        QueryValidatorDelegationsResponse, QueryValidatorRequest, QueryValidatorResponse,
        QueryValidatorUnbondingDelegationsRequest, QueryValidatorUnbondingDelegationsResponse,
        QueryValidatorsRequest, QueryValidatorsResponse,
    },
    google::protobuf::Duration,
};
use ibc_proto::{
    cosmos::{
        base::tendermint::v1beta1::{
            service_server::{Service as HealthService, ServiceServer as HealthServer},
            AbciQueryRequest, AbciQueryResponse, GetBlockByHeightRequest, GetBlockByHeightResponse,
            GetLatestBlockRequest, GetLatestBlockResponse, GetLatestValidatorSetRequest,
            GetLatestValidatorSetResponse, GetNodeInfoRequest, GetNodeInfoResponse,
            GetSyncingRequest, GetSyncingResponse, GetValidatorSetByHeightRequest,
            GetValidatorSetByHeightResponse, Module as VersionInfoModule, VersionInfo,
        },
        tx::v1beta1::{
            service_server::{Service as TxService, ServiceServer as TxServer},
            BroadcastTxRequest, BroadcastTxResponse, GetBlockWithTxsRequest,
            GetBlockWithTxsResponse, GetTxRequest, GetTxResponse, GetTxsEventRequest,
            GetTxsEventResponse, SimulateRequest, SimulateResponse,
        },
    },
    google::protobuf::Any,
};
use tonic::{Request, Response, Status};

use super::Ibc;

impl From<crate::Error> for tonic::Status {
    fn from(err: crate::Error) -> Self {
        tonic::Status::aborted(err.to_string())
    }
}

pub struct IbcClientService {
    ibc: Client<Ibc>,
}

impl IbcClientService {
    pub fn new(ibc: Client<Ibc>) -> Self {
        Self { ibc }
    }
}

#[tonic::async_trait]
impl ClientQuery for IbcClientService {
    async fn client_state(
        &self,
        _request: Request<QueryClientStateRequest>,
    ) -> Result<Response<QueryClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn client_states(
        &self,
        _request: Request<QueryClientStatesRequest>,
    ) -> Result<Response<QueryClientStatesResponse>, Status> {
        let res = QueryClientStatesResponse {
            client_states: self.ibc.query(|ibc| ibc.query_client_states()).await?,
            ..Default::default()
        };

        Ok(Response::new(res))
    }

    async fn consensus_state(
        &self,
        _request: Request<QueryConsensusStateRequest>,
    ) -> Result<Response<QueryConsensusStateResponse>, Status> {
        unimplemented!()
    }

    async fn consensus_states(
        &self,
        request: Request<QueryConsensusStatesRequest>,
    ) -> Result<Response<QueryConsensusStatesResponse>, Status> {
        let client_id: ClientId = request
            .into_inner()
            .client_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid client ID".to_string()))?;

        let consensus_states = self
            .ibc
            .query(|ibc| ibc.query_consensus_states(client_id.clone().into()))
            .await?;

        Ok(Response::new(QueryConsensusStatesResponse {
            consensus_states,
            ..Default::default()
        }))
    }

    async fn consensus_state_heights(
        &self,
        _request: Request<QueryConsensusStateHeightsRequest>,
    ) -> Result<Response<QueryConsensusStateHeightsResponse>, Status> {
        unimplemented!()
    }

    async fn client_status(
        &self,
        _request: Request<QueryClientStatusRequest>,
    ) -> Result<Response<QueryClientStatusResponse>, Status> {
        unimplemented!()
    }

    async fn client_params(
        &self,
        _request: Request<QueryClientParamsRequest>,
    ) -> Result<Response<QueryClientParamsResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_client_state(
        &self,
        _request: Request<QueryUpgradedClientStateRequest>,
    ) -> Result<Response<QueryUpgradedClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn upgraded_consensus_state(
        &self,
        _request: Request<QueryUpgradedConsensusStateRequest>,
    ) -> Result<Response<QueryUpgradedConsensusStateResponse>, Status> {
        unimplemented!()
    }
}

pub struct IbcConnectionService {
    ibc: Client<Ibc>,
}

impl IbcConnectionService {
    pub fn new(ibc: Client<Ibc>) -> Self {
        Self { ibc }
    }
}

#[tonic::async_trait]
impl ConnectionQuery for IbcConnectionService {
    async fn connection(
        &self,
        request: Request<QueryConnectionRequest>,
    ) -> Result<Response<QueryConnectionResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.into_inner().connection_id)
            .map_err(|_| Status::invalid_argument("Invalid connection ID".to_string()))?;

        let conn = self
            .ibc
            .query(|ibc| ibc.query_connection(conn_id.clone().into()))
            .await?;

        Ok(Response::new(QueryConnectionResponse {
            connection: Some(conn.into()),
            ..Default::default()
        }))
    }

    async fn connections(
        &self,
        _request: Request<QueryConnectionsRequest>,
    ) -> Result<Response<QueryConnectionsResponse>, Status> {
        let connections = self.ibc.query(|ibc| ibc.query_all_connections()).await?;

        Ok(Response::new(QueryConnectionsResponse {
            connections,
            ..Default::default()
        }))
    }

    async fn client_connections(
        &self,
        request: Request<QueryClientConnectionsRequest>,
    ) -> Result<Response<QueryClientConnectionsResponse>, Status> {
        let client_id: ClientId = request
            .into_inner()
            .client_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid client ID".to_string()))?;
        let connection_ids = self
            .ibc
            .query(|ibc| ibc.query_client_connections(client_id.clone().into()))
            .await?;

        Ok(Response::new(QueryClientConnectionsResponse {
            connection_paths: connection_ids.into_iter().map(|v| v.to_string()).collect(),
            ..Default::default()
        }))
    }

    async fn connection_client_state(
        &self,
        _request: Request<QueryConnectionClientStateRequest>,
    ) -> Result<Response<QueryConnectionClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn connection_consensus_state(
        &self,
        _request: Request<QueryConnectionConsensusStateRequest>,
    ) -> Result<Response<QueryConnectionConsensusStateResponse>, Status> {
        unimplemented!()
    }
}

pub struct IbcChannelService {}

impl IbcChannelService {
    pub fn new() -> Self {
        Self {}
    }
}

#[tonic::async_trait]
impl ChannelQuery for IbcChannelService {
    async fn channel(
        &self,
        request: Request<QueryChannelRequest>,
    ) -> Result<Response<QueryChannelResponse>, Status> {
        todo!()
    }

    async fn channels(
        &self,
        _request: Request<QueryChannelsRequest>,
    ) -> Result<Response<QueryChannelsResponse>, Status> {
        todo!()
    }

    async fn connection_channels(
        &self,
        request: Request<QueryConnectionChannelsRequest>,
    ) -> Result<Response<QueryConnectionChannelsResponse>, Status> {
        todo!()
    }

    async fn channel_client_state(
        &self,
        _request: Request<QueryChannelClientStateRequest>,
    ) -> Result<Response<QueryChannelClientStateResponse>, Status> {
        unimplemented!()
    }

    async fn channel_consensus_state(
        &self,
        _request: Request<QueryChannelConsensusStateRequest>,
    ) -> Result<Response<QueryChannelConsensusStateResponse>, Status> {
        unimplemented!()
    }

    async fn packet_commitment(
        &self,
        _request: Request<QueryPacketCommitmentRequest>,
    ) -> Result<Response<QueryPacketCommitmentResponse>, Status> {
        unimplemented!()
    }

    async fn packet_commitments(
        &self,
        request: Request<QueryPacketCommitmentsRequest>,
    ) -> Result<Response<QueryPacketCommitmentsResponse>, Status> {
        todo!()
    }

    async fn packet_receipt(
        &self,
        _request: Request<QueryPacketReceiptRequest>,
    ) -> Result<Response<QueryPacketReceiptResponse>, Status> {
        unimplemented!()
    }

    async fn packet_acknowledgement(
        &self,
        _request: Request<QueryPacketAcknowledgementRequest>,
    ) -> Result<Response<QueryPacketAcknowledgementResponse>, Status> {
        unimplemented!()
    }

    async fn packet_acknowledgements(
        &self,
        request: Request<QueryPacketAcknowledgementsRequest>,
    ) -> Result<Response<QueryPacketAcknowledgementsResponse>, Status> {
        unimplemented!()
    }

    async fn unreceived_packets(
        &self,
        request: Request<QueryUnreceivedPacketsRequest>,
    ) -> Result<Response<QueryUnreceivedPacketsResponse>, Status> {
        todo!()
    }

    async fn unreceived_acks(
        &self,
        request: Request<QueryUnreceivedAcksRequest>,
    ) -> Result<Response<QueryUnreceivedAcksResponse>, Status> {
        todo!()
    }

    async fn next_sequence_receive(
        &self,
        _request: Request<QueryNextSequenceReceiveRequest>,
    ) -> Result<Response<QueryNextSequenceReceiveResponse>, Status> {
        unimplemented!()
    }
}

pub struct StakingService {}

#[tonic::async_trait]
impl StakingQuery for StakingService {
    async fn validators(
        &self,
        _request: Request<QueryValidatorsRequest>,
    ) -> Result<Response<QueryValidatorsResponse>, Status> {
        unimplemented!()
    }

    async fn validator(
        &self,
        _request: Request<QueryValidatorRequest>,
    ) -> Result<Response<QueryValidatorResponse>, Status> {
        unimplemented!()
    }

    async fn validator_delegations(
        &self,
        _request: Request<QueryValidatorDelegationsRequest>,
    ) -> Result<Response<QueryValidatorDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn validator_unbonding_delegations(
        &self,
        _request: Request<QueryValidatorUnbondingDelegationsRequest>,
    ) -> Result<Response<QueryValidatorUnbondingDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn delegation(
        &self,
        _request: Request<QueryDelegationRequest>,
    ) -> Result<Response<QueryDelegationResponse>, Status> {
        unimplemented!()
    }

    async fn unbonding_delegation(
        &self,
        _request: Request<QueryUnbondingDelegationRequest>,
    ) -> Result<Response<QueryUnbondingDelegationResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_delegations(
        &self,
        _request: Request<QueryDelegatorDelegationsRequest>,
    ) -> Result<Response<QueryDelegatorDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_unbonding_delegations(
        &self,
        _request: Request<QueryDelegatorUnbondingDelegationsRequest>,
    ) -> Result<Response<QueryDelegatorUnbondingDelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn redelegations(
        &self,
        _request: Request<QueryRedelegationsRequest>,
    ) -> Result<Response<QueryRedelegationsResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_validators(
        &self,
        _request: Request<QueryDelegatorValidatorsRequest>,
    ) -> Result<Response<QueryDelegatorValidatorsResponse>, Status> {
        unimplemented!()
    }

    async fn delegator_validator(
        &self,
        _request: Request<QueryDelegatorValidatorRequest>,
    ) -> Result<Response<QueryDelegatorValidatorResponse>, Status> {
        unimplemented!()
    }

    async fn historical_info(
        &self,
        _request: Request<QueryHistoricalInfoRequest>,
    ) -> Result<Response<QueryHistoricalInfoResponse>, Status> {
        unimplemented!()
    }

    async fn pool(
        &self,
        _request: Request<QueryPoolRequest>,
    ) -> Result<Response<QueryPoolResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<StakingQueryParamsRequest>,
    ) -> Result<Response<StakingQueryParamsResponse>, Status> {
        Ok(Response::new(StakingQueryParamsResponse {
            params: Some(Params {
                unbonding_time: Some(Duration {
                    seconds: 2 * 7 * 24 * 60 * 60,
                    nanos: 0,
                }),
                historical_entries: 1,
                ..Params::default()
            }),
        }))
    }
}

pub struct AuthService {}

#[tonic::async_trait]
impl AuthQuery for AuthService {
    async fn accounts(
        &self,
        _request: Request<QueryAccountsRequest>,
    ) -> Result<Response<QueryAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn account(
        &self,
        _request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        todo!()
    }

    async fn params(
        &self,
        _request: Request<AuthQueryParamsRequest>,
    ) -> Result<Response<AuthQueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn account_address_by_id(
        &self,
        _request: Request<QueryAccountAddressByIdRequest>,
    ) -> Result<Response<QueryAccountAddressByIdResponse>, Status> {
        unimplemented!()
    }

    async fn module_accounts(
        &self,
        _request: Request<QueryModuleAccountsRequest>,
    ) -> Result<Response<QueryModuleAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn module_account_by_name(
        &self,
        _request: Request<QueryModuleAccountByNameRequest>,
    ) -> Result<Response<QueryModuleAccountByNameResponse>, Status> {
        unimplemented!()
    }

    async fn bech32_prefix(
        &self,
        _request: Request<Bech32PrefixRequest>,
    ) -> Result<Response<Bech32PrefixResponse>, Status> {
        unimplemented!()
    }

    async fn address_bytes_to_string(
        &self,
        _request: Request<AddressBytesToStringRequest>,
    ) -> Result<Response<AddressBytesToStringResponse>, Status> {
        unimplemented!()
    }

    async fn address_string_to_bytes(
        &self,
        _request: Request<AddressStringToBytesRequest>,
    ) -> Result<Response<AddressStringToBytesResponse>, Status> {
        unimplemented!()
    }
}

pub struct BankService {}

#[tonic::async_trait]
impl BankQuery for BankService {
    async fn balance(
        &self,
        request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        todo!()
    }

    async fn all_balances(
        &self,
        _request: Request<QueryAllBalancesRequest>,
    ) -> Result<Response<QueryAllBalancesResponse>, Status> {
        unimplemented!()
    }

    async fn spendable_balances(
        &self,
        _request: Request<QuerySpendableBalancesRequest>,
    ) -> Result<Response<QuerySpendableBalancesResponse>, Status> {
        unimplemented!()
    }

    async fn total_supply(
        &self,
        _request: Request<QueryTotalSupplyRequest>,
    ) -> Result<Response<QueryTotalSupplyResponse>, Status> {
        unimplemented!()
    }

    async fn supply_of(
        &self,
        _request: Request<QuerySupplyOfRequest>,
    ) -> Result<Response<QuerySupplyOfResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn denom_metadata(
        &self,
        _request: Request<QueryDenomMetadataRequest>,
    ) -> Result<Response<QueryDenomMetadataResponse>, Status> {
        unimplemented!()
    }

    async fn denoms_metadata(
        &self,
        _request: Request<QueryDenomsMetadataRequest>,
    ) -> Result<Response<QueryDenomsMetadataResponse>, Status> {
        unimplemented!()
    }

    async fn denom_owners(
        &self,
        _request: Request<QueryDenomOwnersRequest>,
    ) -> Result<Response<QueryDenomOwnersResponse>, Status> {
        unimplemented!()
    }
}

pub struct AppHealthService {}

#[tonic::async_trait]
impl HealthService for AppHealthService {
    async fn abci_query(
        &self,
        _request: Request<AbciQueryRequest>,
    ) -> Result<Response<AbciQueryResponse>, Status> {
        unimplemented!()
    }

    async fn get_node_info(
        &self,
        _request: Request<GetNodeInfoRequest>,
    ) -> Result<Response<GetNodeInfoResponse>, Status> {
        todo!()
    }

    async fn get_syncing(
        &self,
        _request: Request<GetSyncingRequest>,
    ) -> Result<Response<GetSyncingResponse>, Status> {
        unimplemented!()
    }

    async fn get_latest_block(
        &self,
        _request: Request<GetLatestBlockRequest>,
    ) -> Result<Response<GetLatestBlockResponse>, Status> {
        unimplemented!()
    }

    async fn get_block_by_height(
        &self,
        _request: Request<GetBlockByHeightRequest>,
    ) -> Result<Response<GetBlockByHeightResponse>, Status> {
        unimplemented!()
    }

    async fn get_latest_validator_set(
        &self,
        _request: Request<GetLatestValidatorSetRequest>,
    ) -> Result<Response<GetLatestValidatorSetResponse>, Status> {
        unimplemented!()
    }

    async fn get_validator_set_by_height(
        &self,
        _request: Request<GetValidatorSetByHeightRequest>,
    ) -> Result<Response<GetValidatorSetByHeightResponse>, Status> {
        unimplemented!()
    }
}

pub struct AppTxService {}

#[tonic::async_trait]
impl TxService for AppTxService {
    async fn simulate(
        &self,
        _request: Request<SimulateRequest>,
    ) -> Result<Response<SimulateResponse>, Status> {
        Ok(Response::new(SimulateResponse {
            gas_info: None,
            result: None,
        }))
    }

    async fn get_tx(
        &self,
        _request: Request<GetTxRequest>,
    ) -> Result<Response<GetTxResponse>, Status> {
        unimplemented!()
    }

    async fn broadcast_tx(
        &self,
        _request: Request<BroadcastTxRequest>,
    ) -> Result<Response<BroadcastTxResponse>, Status> {
        unimplemented!()
    }

    async fn get_txs_event(
        &self,
        _request: Request<GetTxsEventRequest>,
    ) -> Result<Response<GetTxsEventResponse>, Status> {
        unimplemented!()
    }

    async fn get_block_with_txs(
        &self,
        _request: Request<GetBlockWithTxsRequest>,
    ) -> Result<Response<GetBlockWithTxsResponse>, Status> {
        unimplemented!()
    }
}

pub struct GrpcOpts {
    pub host: String,
    pub port: u16,
}

pub async fn start_grpc(client: Client<Ibc>, opts: &GrpcOpts) {
    use tonic::transport::Server;
    let auth_service = AuthQueryServer::new(AuthService {});
    let bank_service = BankQueryServer::new(BankService {});
    let staking_service = StakingQueryServer::new(StakingService {});
    let ibc_client_service = ClientQueryServer::new(IbcClientService::new(client.clone()));
    let ibc_connection_service = ConnectionQueryServer::new(IbcConnectionService::new(client));
    let ibc_channel_service = ChannelQueryServer::new(IbcChannelService {});
    let health_service = HealthServer::new(AppHealthService {});
    let tx_service = TxServer::new(AppTxService {});

    Server::builder()
        .add_service(health_service)
        .add_service(tx_service)
        .add_service(ibc_client_service)
        .add_service(ibc_connection_service)
        .add_service(ibc_channel_service)
        .add_service(auth_service)
        .add_service(bank_service)
        .add_service(staking_service)
        .serve(format!("{}:{}", opts.host, opts.port).parse().unwrap())
        .await
        .unwrap();
}

pub struct Client<T> {
    inner: PhantomData<T>,
}
impl<T> Clone for Client<T> {
    fn clone(&self) -> Self {
        Self { inner: PhantomData }
    }
}

impl<T> Client<T> {
    pub async fn query<F: Fn(T) -> crate::Result<U>, U>(&self, op: F) -> crate::Result<U> {
        todo!()
    }
}

unsafe impl<T> Send for Client<T> {}
unsafe impl<T> Sync for Client<T> {}
