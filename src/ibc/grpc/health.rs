use ibc_proto::cosmos::base::tendermint::v1beta1::{
    service_server::Service as HealthService, GetBlockByHeightRequest, GetBlockByHeightResponse,
    GetLatestBlockRequest, GetLatestBlockResponse, GetLatestValidatorSetRequest,
    GetLatestValidatorSetResponse, GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest,
    GetSyncingResponse, GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse,
};
use tendermint_proto::p2p::DefaultNodeInfo;

use super::Ibc;
use crate::abci::tendermint_client::TendermintAdapter;
use crate::client::{AsyncQuery, Client};
use std::rc::Rc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T, U> HealthService for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
{
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
        dbg!("get_syncing");

        unimplemented!()
    }

    async fn get_latest_block(
        &self,
        _request: Request<GetLatestBlockRequest>,
    ) -> Result<Response<GetLatestBlockResponse>, Status> {
        dbg!("get_latest_block");
        unimplemented!()
    }

    async fn get_block_by_height(
        &self,
        _request: Request<GetBlockByHeightRequest>,
    ) -> Result<Response<GetBlockByHeightResponse>, Status> {
        dbg!("get_block_by_height");
        unimplemented!()
    }

    async fn get_latest_validator_set(
        &self,
        _request: Request<GetLatestValidatorSetRequest>,
    ) -> Result<Response<GetLatestValidatorSetResponse>, Status> {
        println!("grpc get latest validator set");
        unimplemented!()
    }

    async fn get_validator_set_by_height(
        &self,
        _request: Request<GetValidatorSetByHeightRequest>,
    ) -> Result<Response<GetValidatorSetByHeightResponse>, Status> {
        println!("grpc get validator set by height");
        unimplemented!()
    }
}
