use ibc::{
    core::ics24_host::identifier::{ChannelId, ConnectionId, PortId},
    Height,
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
    client::v1::Height as RawHeight,
};

use super::Ibc;
use crate::abci::tendermint_client::{TendermintAdapter, TendermintClient};
use crate::client::{AsyncCall, AsyncQuery, Call, Client};
use crate::query::Query;
use std::{rc::Rc, str::FromStr};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T, U> ChannelQuery for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
{
    async fn channel(
        &self,
        request: Request<QueryChannelRequest>,
    ) -> Result<Response<QueryChannelResponse>, Status> {
        let request = request.into_inner();
        let port_id = PortId::from_str(&request.port_id)
            .map_err(|_| Status::invalid_argument("invalid port id"))?;
        let channel_id = ChannelId::from_str(&request.channel_id)
            .map_err(|_| Status::invalid_argument("invalid channel id"))?;

        let channel = self
            .ibc
            .channels
            .query_channel((port_id, channel_id).into())
            .await?
            .ok();

        Ok(Response::new(QueryChannelResponse {
            channel,
            proof: vec![],
            proof_height: None,
        }))
    }
    async fn channels(
        &self,
        _request: Request<QueryChannelsRequest>,
    ) -> Result<Response<QueryChannelsResponse>, Status> {
        let channels = self.ibc.channels.query_channels().await??;

        Ok(Response::new(QueryChannelsResponse {
            channels,
            pagination: None,
            height: Some(RawHeight {
                revision_height: self.height().await,
                revision_number: 0,
            }), // TODO
        }))
    }
    async fn connection_channels(
        &self,
        request: Request<QueryConnectionChannelsRequest>,
    ) -> Result<Response<QueryConnectionChannelsResponse>, Status> {
        let conn_id = ConnectionId::from_str(&request.get_ref().connection)
            .map_err(|_| Status::invalid_argument("invalid connection id"))?;

        let channels = self
            .ibc
            .channels
            .query_connection_channels(conn_id.into())
            .await??;

        Ok(Response::new(QueryConnectionChannelsResponse {
            channels,
            pagination: None,
            height: Some(RawHeight {
                revision_height: self.height().await,
                revision_number: 0,
            }), // TODO
        }))
    }

    async fn channel_client_state(
        &self,
        _request: Request<QueryChannelClientStateRequest>,
    ) -> Result<Response<QueryChannelClientStateResponse>, Status> {
        todo!()
    }

    async fn channel_consensus_state(
        &self,
        _request: Request<QueryChannelConsensusStateRequest>,
    ) -> Result<Response<QueryChannelConsensusStateResponse>, Status> {
        todo!()
    }

    async fn packet_commitment(
        &self,
        _request: Request<QueryPacketCommitmentRequest>,
    ) -> Result<Response<QueryPacketCommitmentResponse>, Status> {
        todo!()
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

        let async_port_id = port_id.clone();
        let async_channel_id = channel_id.clone();

        //TODO: combine commitment queries
        let commitments = self
            .ibc
            .channels
            .packet_commitments((port_id.clone(), channel_id.clone()).into())
            .await??;

        let transfer_commitments_response = self
            .ibc_with_height(async move |client| {
                Fn::call(&self.ibc_provider, (client,))
                    .transfers
                    .packet_commitments((async_port_id, async_channel_id).into())
                    .await
            })
            .await?;

        let commitments = [commitments, transfer_commitments_response.0?].concat();

        Ok(Response::new(QueryPacketCommitmentsResponse {
            commitments,
            pagination: None,
            height: Some(RawHeight {
                revision_height: transfer_commitments_response.1,
                revision_number: 0,
            }),
        }))
    }

    async fn packet_receipt(
        &self,
        _request: Request<QueryPacketReceiptRequest>,
    ) -> Result<Response<QueryPacketReceiptResponse>, Status> {
        todo!()
    }

    async fn packet_acknowledgement(
        &self,
        _request: Request<QueryPacketAcknowledgementRequest>,
    ) -> Result<Response<QueryPacketAcknowledgementResponse>, Status> {
        todo!()
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

        let acknowledgements = self
            .ibc
            .channels
            .query_packet_acks((port_id, channel_id).into())
            .await?;

        Ok(Response::new(QueryPacketAcknowledgementsResponse {
            acknowledgements,
            pagination: None,
            height: Some(RawHeight {
                revision_number: 0,
                revision_height: self.height().await,
            }), // TODO
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

        let unreceived_sequences: Vec<u64> = self
            .ibc
            .channels
            .query_unreceived_packets((port_id, channel_id).into(), sequences_to_check.into())
            .await?;

        Ok(Response::new(QueryUnreceivedPacketsResponse {
            sequences: unreceived_sequences,
            height: Some(RawHeight {
                revision_number: 0,
                revision_height: self.height().await,
            }), // TODO
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

        let unreceived_sequences: Vec<u64> = self
            .ibc
            .channels
            .query_unreceived_acks((port_id, channel_id).into(), sequences_to_check.into())
            .await?;

        Ok(Response::new(QueryUnreceivedAcksResponse {
            sequences: unreceived_sequences,
            height: Some(RawHeight {
                revision_number: 0,
                revision_height: self.height().await,
            }), // TODO
        }))
    }

    async fn next_sequence_receive(
        &self,
        _request: Request<QueryNextSequenceReceiveRequest>,
    ) -> Result<Response<QueryNextSequenceReceiveResponse>, Status> {
        todo!()
    }
}
