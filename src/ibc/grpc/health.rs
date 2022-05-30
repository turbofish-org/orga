use cosmos_sdk_proto::cosmos::base::tendermint::v1beta1::service_server::Service as HealthService;
use cosmos_sdk_proto::cosmos::base::tendermint::v1beta1::{
    GetBlockByHeightRequest, GetBlockByHeightResponse, GetLatestBlockRequest,
    GetLatestBlockResponse, GetLatestValidatorSetRequest, GetLatestValidatorSetResponse,
    GetNodeInfoRequest, GetNodeInfoResponse, GetSyncingRequest, GetSyncingResponse,
    GetValidatorSetByHeightRequest, GetValidatorSetByHeightResponse, Module as VersionInfoModule,
    VersionInfo,
};

use super::Ibc;
use crate::client::{AsyncCall, AsyncQuery, Call};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T> HealthService for super::GrpcServer<T>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: AsyncQuery<Response = Ibc>,
{
    async fn get_node_info(
        &self,
        _request: Request<GetNodeInfoRequest>,
    ) -> Result<Response<GetNodeInfoResponse>, Status> {
        dbg!("get_node_info");
        unimplemented!()
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
        unimplemented!()
    }

    async fn get_validator_set_by_height(
        &self,
        _request: Request<GetValidatorSetByHeightRequest>,
    ) -> Result<Response<GetValidatorSetByHeightResponse>, Status> {
        unimplemented!()
    }
}
