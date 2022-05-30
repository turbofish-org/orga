use cosmos_sdk_proto::ibc::core::client::v1::IdentifiedClientState;
use cosmos_sdk_proto::ibc::core::client::v1::{
    query_server::Query as ClientQuery, ConsensusStateWithHeight, Height as RawHeight,
    QueryClientParamsRequest, QueryClientParamsResponse, QueryClientStateRequest,
    QueryClientStateResponse, QueryClientStatesRequest, QueryClientStatesResponse,
    QueryClientStatusRequest, QueryClientStatusResponse, QueryConsensusStateRequest,
    QueryConsensusStateResponse, QueryConsensusStatesRequest, QueryConsensusStatesResponse,
    QueryUpgradedClientStateRequest, QueryUpgradedClientStateResponse,
    QueryUpgradedConsensusStateRequest, QueryUpgradedConsensusStateResponse,
};

use super::Ibc;
use crate::client::{AsyncCall, AsyncQuery, Call};
use crate::query::Query;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T> ClientQuery for super::GrpcServer<T>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: AsyncQuery<Response = Ibc>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
{
    async fn client_state(
        &self,
        _request: Request<QueryClientStateRequest>,
    ) -> Result<Response<QueryClientStateResponse>, Status> {
        println!("query client state");
        unimplemented!()
    }

    async fn client_states(
        &self,
        _request: Request<QueryClientStatesRequest>,
    ) -> Result<Response<QueryClientStatesResponse>, Status> {
        println!("query client states");
        dbg!(&_request);
        let mut res = QueryClientStatesResponse::default();

        res.client_states.push(IdentifiedClientState {
            client_id: "client_id".to_string(),
            client_state: None,
        });
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
        _request: Request<QueryConsensusStatesRequest>,
    ) -> Result<Response<QueryConsensusStatesResponse>, Status> {
        todo!()
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
