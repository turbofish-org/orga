use ibc_proto::ibc::core::client::v1::{
    query_server::Query as ClientQuery, QueryClientParamsRequest, QueryClientParamsResponse,
    QueryClientStateRequest, QueryClientStateResponse, QueryClientStatesRequest,
    QueryClientStatesResponse, QueryClientStatusRequest, QueryClientStatusResponse,
    QueryConsensusStateRequest, QueryConsensusStateResponse, QueryConsensusStatesRequest,
    QueryConsensusStatesResponse, QueryUpgradedClientStateRequest,
    QueryUpgradedClientStateResponse, QueryUpgradedConsensusStateRequest,
    QueryUpgradedConsensusStateResponse,
};
use ibc_proto::ibc::core::client::v1::{
    QueryConsensusStateHeightsRequest, QueryConsensusStateHeightsResponse,
};

use super::Ibc;
use crate::abci::tendermint_client::TendermintAdapter;
use crate::client::{AsyncQuery, Client};
use crate::query::Query;
use std::rc::Rc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T, U> ClientQuery for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
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
        let res = QueryClientStatesResponse {
            client_states: self.ibc.clients.query_client_states().await??,
            ..Default::default()
        };

        Ok(Response::new(res))
    }

    async fn consensus_state(
        &self,
        _request: Request<QueryConsensusStateRequest>,
    ) -> Result<Response<QueryConsensusStateResponse>, Status> {
        println!("grpc consensus state");
        unimplemented!()
    }

    async fn consensus_state_heights(
        &self,
        _request: Request<QueryConsensusStateHeightsRequest>,
    ) -> Result<Response<QueryConsensusStateHeightsResponse>, Status> {
        println!("grpc consensus state heights");
        todo!()
    }

    async fn consensus_states(
        &self,
        request: Request<QueryConsensusStatesRequest>,
    ) -> Result<Response<QueryConsensusStatesResponse>, Status> {
        println!("grpc consensus states");
        use ibc::core::ics24_host::Path;
        let path: Path = format!("clients/{}/consensusStates", request.get_ref().client_id)
            .parse()
            .map_err(|e| Status::invalid_argument(format!("{}", e)))?;
        if let Path::ClientConsensusState(data) = path {
            let client_id = data.client_id;

            let consensus_states = self
                .ibc
                .clients
                .query_consensus_states(client_id.into())
                .await??;
            Ok(Response::new(QueryConsensusStatesResponse {
                consensus_states,
                pagination: None,
            }))
        } else {
            Err(Status::invalid_argument(
                "Could not fetch client consensus states",
            ))
        }
    }

    async fn client_status(
        &self,
        _request: Request<QueryClientStatusRequest>,
    ) -> Result<Response<QueryClientStatusResponse>, Status> {
        println!("grpc client status");
        unimplemented!()
    }

    async fn client_params(
        &self,
        _request: Request<QueryClientParamsRequest>,
    ) -> Result<Response<QueryClientParamsResponse>, Status> {
        println!("grpc client params");
        unimplemented!()
    }

    async fn upgraded_client_state(
        &self,
        _request: Request<QueryUpgradedClientStateRequest>,
    ) -> Result<Response<QueryUpgradedClientStateResponse>, Status> {
        println!("grpc upgraded client state");
        unimplemented!()
    }

    async fn upgraded_consensus_state(
        &self,
        _request: Request<QueryUpgradedConsensusStateRequest>,
    ) -> Result<Response<QueryUpgradedConsensusStateResponse>, Status> {
        println!("grpc upgraded consensus state");
        unimplemented!()
    }
}
