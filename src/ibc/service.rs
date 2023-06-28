use std::fmt::Debug;
use std::str::FromStr;

use futures_lite::Future;
use ibc::core::ics24_host::identifier::{ClientId, ConnectionId, PortId};
use ibc::core::ics24_host::{identifier::ChannelId, path::ChannelEndPath};

use ibc_proto::cosmos::auth::v1beta1::{
    query_server::Query as AuthQuery, query_server::QueryServer as AuthQueryServer,
    AddressBytesToStringRequest, AddressBytesToStringResponse, AddressStringToBytesRequest,
    AddressStringToBytesResponse, BaseAccount, Bech32PrefixRequest, Bech32PrefixResponse,
    QueryAccountAddressByIdRequest, QueryAccountAddressByIdResponse, QueryAccountRequest,
    QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
    QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse, QueryModuleAccountsRequest,
    QueryModuleAccountsResponse, QueryParamsRequest as AuthQueryParamsRequest,
    QueryParamsResponse as AuthQueryParamsResponse,
};
use ibc_proto::cosmos::bank::v1beta1::{
    query_server::{Query as BankQuery, QueryServer as BankQueryServer},
    QueryAllBalancesRequest, QueryAllBalancesResponse, QueryBalanceRequest, QueryBalanceResponse,
    QueryDenomMetadataRequest, QueryDenomMetadataResponse, QueryDenomOwnersRequest,
    QueryDenomOwnersResponse, QueryDenomsMetadataRequest, QueryDenomsMetadataResponse,
    QueryParamsRequest, QueryParamsResponse, QuerySpendableBalancesRequest,
    QuerySpendableBalancesResponse, QuerySupplyOfRequest, QuerySupplyOfResponse,
    QueryTotalSupplyRequest, QueryTotalSupplyResponse,
};
use ibc_proto::ibc::core::channel::v1::{Channel, IdentifiedChannel, PacketState};
use ibc_proto::ibc::core::client::v1::{ConsensusStateWithHeight, IdentifiedClientState};
use ibc_proto::ibc::core::connection::v1::IdentifiedConnection;
use ibc_proto::ibc::core::{
    channel::v1::{
        query_server::{Query as ChannelQuery, QueryServer as ChannelQueryServer},
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
            GetValidatorSetByHeightResponse,
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
use prost::Message;
use tendermint_proto::p2p::DefaultNodeInfo;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use crate::client::Client;

use super::{ConnectionEnd, Ibc, PortChannel};

impl From<crate::Error> for tonic::Status {
    fn from(err: crate::Error) -> Self {
        tonic::Status::aborted(err.to_string())
    }
}

pub struct IbcClientService {
    client_states: Actor<(), Vec<IdentifiedClientState>>,
    consensus_states: Actor<ClientId, Vec<ConsensusStateWithHeight>>,
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
            client_states: self.client_states.req(()).await,
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

        let res = QueryConsensusStatesResponse {
            consensus_states: self.consensus_states.req(client_id).await,
            ..Default::default()
        };
        Ok(Response::new(res))
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
    connection: Actor<ConnectionId, Option<ConnectionEnd>>,
    connections: Actor<(), Vec<IdentifiedConnection>>,
    client_connections: Actor<ClientId, Vec<super::ConnectionId>>,
}

#[tonic::async_trait]
impl ConnectionQuery for IbcConnectionService {
    async fn connection(
        &self,
        request: Request<QueryConnectionRequest>,
    ) -> Result<Response<QueryConnectionResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.into_inner().connection_id)
            .map_err(|_| Status::invalid_argument("Invalid connection ID".to_string()))?;

        Ok(Response::new(QueryConnectionResponse {
            connection: self.connection.req(conn_id).await.map(Into::into),
            ..Default::default()
        }))
    }

    async fn connections(
        &self,
        _request: Request<QueryConnectionsRequest>,
    ) -> Result<Response<QueryConnectionsResponse>, Status> {
        Ok(Response::new(QueryConnectionsResponse {
            connections: self.connections.req(()).await,
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
        let connection_ids = self.client_connections.req(client_id).await;

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

pub struct IbcChannelService {
    channel: Actor<ChannelEndPath, Option<Channel>>,
    channels: Actor<(), Vec<IdentifiedChannel>>,
    connection_channels: Actor<ConnectionId, Vec<IdentifiedChannel>>,
    packet_commitments: Actor<PortChannel, Vec<PacketState>>,
    packet_acks: Actor<PortChannel, Vec<PacketState>>,
    unreceived_packets: Actor<(PortChannel, Vec<u64>), Vec<u64>>,
    unreceived_acks: Actor<(PortChannel, Vec<u64>), Vec<u64>>,
}

#[tonic::async_trait]
impl ChannelQuery for IbcChannelService {
    async fn channel(
        &self,
        request: Request<QueryChannelRequest>,
    ) -> Result<Response<QueryChannelResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let path = ChannelEndPath(port_id, channel_id);

        Ok(Response::new(QueryChannelResponse {
            channel: self.channel.req(path).await,
            ..Default::default()
        }))
    }

    async fn channels(
        &self,
        _request: Request<QueryChannelsRequest>,
    ) -> Result<Response<QueryChannelsResponse>, Status> {
        Ok(Response::new(QueryChannelsResponse {
            channels: self.channels.req(()).await,
            ..Default::default()
        }))
    }

    async fn connection_channels(
        &self,
        request: Request<QueryConnectionChannelsRequest>,
    ) -> Result<Response<QueryConnectionChannelsResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.get_ref().connection)
            .map_err(|_| Status::invalid_argument("invalid connection id"))?;

        Ok(Response::new(QueryConnectionChannelsResponse {
            channels: self.connection_channels.req(conn_id).await,
            ..Default::default()
        }))
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
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let path = PortChannel::new(port_id, channel_id);

        Ok(Response::new(QueryPacketCommitmentsResponse {
            commitments: self.packet_commitments.req(path).await,
            ..Default::default()
        }))
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
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let path = PortChannel::new(port_id, channel_id);

        Ok(Response::new(QueryPacketAcknowledgementsResponse {
            acknowledgements: self.packet_acks.req(path).await,
            ..Default::default()
        }))
    }

    async fn unreceived_packets(
        &self,
        request: Request<QueryUnreceivedPacketsRequest>,
    ) -> Result<Response<QueryUnreceivedPacketsResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;
        let sequences_to_check: Vec<u64> = request.packet_commitment_sequences;
        let path = PortChannel::new(port_id, channel_id);

        Ok(Response::new(QueryUnreceivedPacketsResponse {
            sequences: self
                .unreceived_packets
                .req((path, sequences_to_check))
                .await,
            ..Default::default()
        }))
    }

    async fn unreceived_acks(
        &self,
        request: Request<QueryUnreceivedAcksRequest>,
    ) -> Result<Response<QueryUnreceivedAcksResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;
        let sequences_to_check: Vec<u64> = request.packet_ack_sequences;
        let path = PortChannel::new(port_id, channel_id);

        Ok(Response::new(QueryUnreceivedAcksResponse {
            sequences: self.unreceived_acks.req((path, sequences_to_check)).await,
            ..Default::default()
        }))
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
        println!("get chain params req");
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
        request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        let account = BaseAccount {
            address: request.into_inner().address,
            ..Default::default()
        };
        Ok(Response::new(QueryAccountResponse {
            account: Some(Any {
                type_url: "/cosmos.auth.v1beta1.BaseAccount".to_string(),
                value: account.encode_to_vec(),
            }),
        }))
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
        _request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        Ok(Response::new(QueryBalanceResponse { balance: None }))
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
        Ok(Response::new(GetNodeInfoResponse {
            default_node_info: Some(DefaultNodeInfo::default()),
            application_version: None,
        }))
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

pub struct Actor<Req, Res> {
    tx_req: tokio::sync::mpsc::Sender<Req>,
    rx_res: Mutex<tokio::sync::mpsc::Receiver<Res>>,
}

impl<Req: Debug + 'static, Res: Debug + 'static> Actor<Req, Res> {
    pub fn new<F: Fn(Req) -> R + 'static, R: Future<Output = Res> + 'static>(f: F) -> Self {
        let (tx_req, mut rx_req) = tokio::sync::mpsc::channel(1);
        let (tx_res, rx_res) = tokio::sync::mpsc::channel(1);

        tokio::task::spawn_local(async move {
            while let Some(req) = rx_req.recv().await {
                let res = f(req).await;
                tx_res.send(res).await.unwrap();
            }
        });

        Self {
            tx_req,
            rx_res: Mutex::new(rx_res),
        }
    }

    pub async fn req(&self, req: Req) -> Res {
        self.tx_req.send(req).await.unwrap();
        self.rx_res.lock().await.recv().await.unwrap()
    }
}

pub async fn start_grpc<C: Client<Ibc> + 'static>(client: fn() -> C, opts: &GrpcOpts) {
    use tonic::transport::Server;
    let auth_service = AuthQueryServer::new(AuthService {});
    let bank_service = BankQueryServer::new(BankService {});
    let staking_service = StakingQueryServer::new(StakingService {});
    let ibc_client_service = ClientQueryServer::new(IbcClientService {
        client_states: Actor::new(async move |_| {
            client()
                .query_sync(|ibc| ibc.query_client_states())
                .unwrap()
        }),
        consensus_states: Actor::new(async move |client_id: ClientId| {
            client()
                .query_sync(|ibc| ibc.query_consensus_states(client_id.clone().into()))
                .unwrap()
        }),
    });
    let ibc_connection_service = ConnectionQueryServer::new(IbcConnectionService {
        connection: Actor::new(async move |conn_id: ConnectionId| {
            client()
                .query_sync(|ibc| ibc.query_connection(conn_id.clone().into()))
                .unwrap()
        }),
        connections: Actor::new(async move |_| {
            client()
                .query_sync(|ibc| ibc.query_all_connections())
                .unwrap()
        }),
        client_connections: Actor::new(async move |client_id: ClientId| {
            client()
                .query_sync(|ibc| ibc.query_client_connections(client_id.clone().into()))
                .unwrap()
        }),
    });
    let ibc_channel_service = ChannelQueryServer::new(IbcChannelService {
        channel: Actor::new(async move |path: ChannelEndPath| {
            client()
                .query_sync(|ibc| ibc.query_channel(path.clone().into()))
                .unwrap()
        }),
        channels: Actor::new(async move |_| {
            client().query_sync(|ibc| ibc.query_all_channels()).unwrap()
        }),
        connection_channels: Actor::new(async move |conn_id: ConnectionId| {
            client()
                .query_sync(|ibc| ibc.query_connection_channels(conn_id.clone().into()))
                .unwrap()
        }),
        packet_commitments: Actor::new(async move |path: PortChannel| {
            client()
                .query_sync(|ibc| ibc.query_packet_commitments(path.clone()))
                .unwrap()
        }),
        packet_acks: Actor::new(async move |path: PortChannel| {
            client()
                .query_sync(|ibc| ibc.query_packet_acks(path.clone()))
                .unwrap()
        }),
        unreceived_packets: Actor::new(async move |(path, seqs): (PortChannel, Vec<u64>)| {
            client()
                .query_sync(|ibc| {
                    ibc.query_unreceived_packets(path.clone(), seqs.clone().try_into().unwrap())
                })
                .unwrap()
        }),
        unreceived_acks: Actor::new(async move |(path, seqs): (PortChannel, Vec<u64>)| {
            client()
                .query_sync(|ibc| {
                    ibc.query_unreceived_acks(path.clone(), seqs.clone().try_into().unwrap())
                })
                .unwrap()
        }),
    });
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

// #[cfg(test)]
// mod tests {
//     use crate::client::{mock::MockClient, wallet::Unsigned, AppClient};

//     use super::*;
//     use crate::coins::Symbol;

//     #[crate::orga]
//     #[derive(Clone, Debug)]
//     struct FooCoin {}

//     impl Symbol for FooCoin {
//         const INDEX: u8 = 123;
//     }

// #[ignore]
// #[tokio::test]
// async fn grpc() {
//     let local = tokio::task::LocalSet::new();
//     local
//         .run_until(async move {
//             start_grpc(
//                 || AppClient::<Ibc, Ibc, _, FooCoin, _>::new(MockClient::default(), Unsigned),
//                 &GrpcOpts {
//                     host: "0.0.0.0".to_string(),
//                     port: 9001,
//                 },
//             )
//             .await
//         })
//         .await

//     // TODO: run a test client against the server
// }
// }
