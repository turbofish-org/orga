use std::str::FromStr;

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
use ibc_proto::cosmos::base::v1beta1::Coin;
use ibc_proto::ibc::core::connection::v1::{
    QueryConnectionParamsRequest, QueryConnectionParamsResponse,
};
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
        Height as RawHeight, QueryClientParamsRequest, QueryClientParamsResponse,
        QueryClientStateRequest, QueryClientStateResponse, QueryClientStatesRequest,
        QueryClientStatesResponse, QueryClientStatusRequest, QueryClientStatusResponse,
        QueryConsensusStateHeightsRequest, QueryConsensusStateHeightsResponse,
        QueryConsensusStateRequest, QueryConsensusStateResponse, QueryConsensusStatesRequest,
        QueryConsensusStatesResponse, QueryUpgradedClientStateRequest,
        QueryUpgradedClientStateResponse, QueryUpgradedConsensusStateRequest,
        QueryUpgradedConsensusStateResponse,
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
use tonic::{Request, Response, Status};

use crate::client::Client;

use super::{IbcContext, PortChannel};

impl From<crate::Error> for tonic::Status {
    fn from(err: crate::Error) -> Self {
        tonic::Status::aborted(err.to_string())
    }
}

pub struct IbcClientService<C> {
    pub ibc: fn() -> C,
}

#[tonic::async_trait]
impl<C: Client<IbcContext> + 'static> ClientQuery for IbcClientService<C> {
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
        let ibc = (self.ibc)();
        tokio::task::spawn_blocking(move || {
            let res = QueryClientStatesResponse {
                client_states: ibc.query_sync(|ibc| ibc.query_client_states())?,
                ..Default::default()
            };
            Ok(Response::new(res))
        })
        .await
        .unwrap()
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
        let ibc = (self.ibc)();
        tokio::task::spawn_blocking(move || {
            let client_id: ClientId = request
                .into_inner()
                .client_id
                .parse()
                .map_err(|_| Status::invalid_argument("Invalid client ID".to_string()))?;

            let res = QueryConsensusStatesResponse {
                consensus_states: ibc
                    .query_sync(|ibc| ibc.query_consensus_states(client_id.clone().into()))?,
                ..Default::default()
            };
            Ok(Response::new(res))
        })
        .await
        .unwrap()
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

pub struct IbcConnectionService<C> {
    ibc: fn() -> C,
}

#[tonic::async_trait]
impl<C: Client<IbcContext> + 'static> ConnectionQuery for IbcConnectionService<C> {
    async fn connection(
        &self,
        request: Request<QueryConnectionRequest>,
    ) -> Result<Response<QueryConnectionResponse>, Status> {
        let ibc = (self.ibc)();
        tokio::task::spawn_blocking(move || {
            let conn_id = ConnectionId::from_str(&request.into_inner().connection_id)
                .map_err(|_| Status::invalid_argument("Invalid connection ID".to_string()))?;

            Ok(Response::new(QueryConnectionResponse {
                connection: ibc
                    .query_sync(|ibc| ibc.query_connection(conn_id.clone().into()))?
                    .map(Into::into),
                ..Default::default()
            }))
        })
        .await
        .unwrap()
    }

    async fn connections(
        &self,
        _request: Request<QueryConnectionsRequest>,
    ) -> Result<Response<QueryConnectionsResponse>, Status> {
        let ibc = (self.ibc)();
        tokio::task::spawn_blocking(move || {
            Ok(Response::new(QueryConnectionsResponse {
                connections: ibc.query_sync(|ibc| ibc.query_all_connections())?,
                ..Default::default()
            }))
        })
        .await
        .unwrap()
    }

    async fn client_connections(
        &self,
        request: Request<QueryClientConnectionsRequest>,
    ) -> Result<Response<QueryClientConnectionsResponse>, Status> {
        let ibc = (self.ibc)();
        tokio::task::spawn_blocking(move || {
            let client_id: ClientId = request
                .into_inner()
                .client_id
                .parse()
                .map_err(|_| Status::invalid_argument("Invalid client ID".to_string()))?;
            let connection_ids =
                ibc.query_sync(|ibc| ibc.query_client_connections(client_id.clone().into()))?;

            Ok(Response::new(QueryClientConnectionsResponse {
                connection_paths: connection_ids.into_iter().map(|v| v.to_string()).collect(),
                ..Default::default()
            }))
        })
        .await
        .unwrap()
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

    async fn connection_params(
        &self,
        _request: Request<QueryConnectionParamsRequest>,
    ) -> Result<Response<QueryConnectionParamsResponse>, Status> {
        unimplemented!()
    }
}

pub struct IbcChannelService<C> {
    ibc: fn() -> C,
    revision_number: u64,
}

#[tonic::async_trait]
impl<C: Client<IbcContext> + 'static> ChannelQuery for IbcChannelService<C> {
    async fn channel(
        &self,
        request: Request<QueryChannelRequest>,
    ) -> Result<Response<QueryChannelResponse>, Status> {
        let ibc = (self.ibc)();
        tokio::task::spawn_blocking(move || {
            let request = request.into_inner();
            let port_id = PortId::from_str(&request.port_id)
                .map_err(|_| Status::invalid_argument("invalid port id"))?;
            let channel_id = ChannelId::from_str(&request.channel_id)
                .map_err(|_| Status::invalid_argument("invalid channel id"))?;

            let path = ChannelEndPath(port_id, channel_id);

            Ok(Response::new(QueryChannelResponse {
                channel: ibc.query_sync(|ibc| ibc.query_channel(path.clone().into()))?,
                ..Default::default()
            }))
        })
        .await
        .unwrap()
    }

    async fn channels(
        &self,
        _request: Request<QueryChannelsRequest>,
    ) -> Result<Response<QueryChannelsResponse>, Status> {
        let ibc = (self.ibc)();
        let revision_number = self.revision_number;
        tokio::task::spawn_blocking(move || {
            let (channels, height) =
                ibc.query_sync(|ibc| Ok((ibc.query_all_channels()?, ibc.height)))?;

            Ok(Response::new(QueryChannelsResponse {
                channels,
                height: Some(RawHeight {
                    revision_number,
                    revision_height: height,
                }),
                ..Default::default()
            }))
        })
        .await
        .unwrap()
    }

    async fn connection_channels(
        &self,
        request: Request<QueryConnectionChannelsRequest>,
    ) -> Result<Response<QueryConnectionChannelsResponse>, Status> {
        let ibc = (self.ibc)();
        let revision_number = self.revision_number;
        tokio::task::spawn_blocking(move || {
            let conn_id = ConnectionId::from_str(&request.get_ref().connection)
                .map_err(|_| Status::invalid_argument("invalid connection id"))?;

            let (channels, height) = ibc.query_sync(|ibc| {
                Ok((
                    ibc.query_connection_channels(conn_id.clone().into())?,
                    ibc.height,
                ))
            })?;
            Ok(Response::new(QueryConnectionChannelsResponse {
                channels,
                height: Some(RawHeight {
                    revision_number,
                    revision_height: height,
                }),
                ..Default::default()
            }))
        })
        .await
        .unwrap()
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
        let ibc = (self.ibc)();
        let revision_number = self.revision_number;
        tokio::task::spawn_blocking(move || {
            let request = request.into_inner();
            let port_id = PortId::from_str(&request.port_id)
                .map_err(|_| Status::invalid_argument("invalid port id"))?;
            let channel_id = ChannelId::from_str(&request.channel_id)
                .map_err(|_| Status::invalid_argument("invalid channel id"))?;

            let path = PortChannel::new(port_id, channel_id);

            let (commitments, height) = ibc
                .query_sync(|ibc| Ok((ibc.query_packet_commitments(path.clone())?, ibc.height)))?;

            Ok(Response::new(QueryPacketCommitmentsResponse {
                commitments,
                height: Some(RawHeight {
                    revision_number,
                    revision_height: height,
                }),
                ..Default::default()
            }))
        })
        .await
        .unwrap()
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
        let ibc = (self.ibc)();
        let revision_number = self.revision_number;
        tokio::task::spawn_blocking(move || {
            let request = request.into_inner();
            let port_id = PortId::from_str(&request.port_id)
                .map_err(|_| Status::invalid_argument("invalid port id"))?;
            let channel_id = ChannelId::from_str(&request.channel_id)
                .map_err(|_| Status::invalid_argument("invalid channel id"))?;
            let sequences = request.packet_commitment_sequences;

            let path = PortChannel::new(port_id, channel_id);
            let (acknowledgements, height) = ibc.query_sync(|ibc| {
                Ok((
                    ibc.query_packet_acks(sequences.clone().try_into().unwrap(), path.clone())?,
                    ibc.height,
                ))
            })?;

            Ok(Response::new(QueryPacketAcknowledgementsResponse {
                acknowledgements,
                height: Some(RawHeight {
                    revision_number,
                    revision_height: height,
                }),
                ..Default::default()
            }))
        })
        .await
        .unwrap()
    }

    async fn unreceived_packets(
        &self,
        request: Request<QueryUnreceivedPacketsRequest>,
    ) -> Result<Response<QueryUnreceivedPacketsResponse>, Status> {
        let ibc = (self.ibc)();
        let revision_number = self.revision_number;
        tokio::task::spawn_blocking(move || {
            let request = request.into_inner();
            let port_id = PortId::from_str(&request.port_id)
                .map_err(|_| Status::invalid_argument("invalid port id"))?;
            let channel_id = ChannelId::from_str(&request.channel_id)
                .map_err(|_| Status::invalid_argument("invalid channel id"))?;
            let sequences_to_check: Vec<u64> = request.packet_commitment_sequences;
            let path = PortChannel::new(port_id, channel_id);

            let (sequences, height) = ibc.query_sync(|ibc| {
                Ok((
                    ibc.query_unreceived_packets(
                        path.clone(),
                        sequences_to_check.clone().try_into().unwrap(),
                    )?,
                    ibc.height,
                ))
            })?;

            Ok(Response::new(QueryUnreceivedPacketsResponse {
                sequences,
                height: Some(RawHeight {
                    revision_number,
                    revision_height: height,
                }),
            }))
        })
        .await
        .unwrap()
    }

    async fn unreceived_acks(
        &self,
        request: Request<QueryUnreceivedAcksRequest>,
    ) -> Result<Response<QueryUnreceivedAcksResponse>, Status> {
        let ibc = (self.ibc)();
        let revision_number = self.revision_number;
        tokio::task::spawn_blocking(move || {
            let request = request.into_inner();
            let port_id = PortId::from_str(&request.port_id)
                .map_err(|_| Status::invalid_argument("invalid port id"))?;
            let channel_id = ChannelId::from_str(&request.channel_id)
                .map_err(|_| Status::invalid_argument("invalid channel id"))?;
            let sequences_to_check: Vec<u64> = request.packet_ack_sequences;
            let path = PortChannel::new(port_id, channel_id);

            let (sequences, height) = ibc.query_sync(|ibc| {
                Ok((
                    ibc.query_unreceived_acks(
                        path.clone(),
                        sequences_to_check.clone().try_into().unwrap(),
                    )?,
                    ibc.height,
                ))
            })?;

            Ok(Response::new(QueryUnreceivedAcksResponse {
                sequences,
                height: Some(RawHeight {
                    revision_number,
                    revision_height: height,
                }),
            }))
        })
        .await
        .unwrap()
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
                historical_entries: 20,
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
        request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        Ok(Response::new(QueryBalanceResponse {
            balance: Some(Coin {
                amount: "0".to_string(),
                denom: request.get_ref().denom.clone(),
            }),
        }))
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
    pub chain_id: String,
}

pub async fn start_grpc<C: Client<IbcContext> + 'static>(client: fn() -> C, opts: &GrpcOpts) {
    use tonic::transport::Server;
    let auth_service = AuthQueryServer::new(AuthService {});
    let bank_service = BankQueryServer::new(BankService {});
    let staking_service = StakingQueryServer::new(StakingService {});
    let ibc_client_service = ClientQueryServer::new(IbcClientService { ibc: client });
    let ibc_connection_service = ConnectionQueryServer::new(IbcConnectionService { ibc: client });
    let revision_number = opts
        .chain_id
        .rsplit_once('-')
        .map(|(_, n)| n.parse::<u64>().unwrap_or(0))
        .unwrap_or(0);
    let ibc_channel_service = ChannelQueryServer::new(IbcChannelService {
        ibc: client,
        revision_number,
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
