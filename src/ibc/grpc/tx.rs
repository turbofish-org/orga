use ibc_proto::cosmos::tx::v1beta1::service_server::Service as TxService;
use ibc_proto::cosmos::tx::v1beta1::{
    BroadcastTxRequest, BroadcastTxResponse, GetBlockWithTxsRequest, GetBlockWithTxsResponse,
    GetTxRequest, GetTxResponse, GetTxsEventRequest, GetTxsEventResponse, SimulateRequest,
    SimulateResponse,
};

use super::Ibc;
use crate::abci::tendermint_client::TendermintAdapter;
use crate::client::{AsyncQuery, Client};
use std::rc::Rc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T, U> TxService for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
{
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
        dbg!("get tx");
        todo!()
    }

    async fn broadcast_tx(
        &self,
        _request: Request<BroadcastTxRequest>,
    ) -> Result<Response<BroadcastTxResponse>, Status> {
        dbg!("broadcast tx");
        todo!()
    }

    async fn get_txs_event(
        &self,
        _request: Request<GetTxsEventRequest>,
    ) -> Result<Response<GetTxsEventResponse>, Status> {
        dbg!("get txs event");
        todo!()
    }

    async fn get_block_with_txs(
        &self,
        _request: Request<GetBlockWithTxsRequest>,
    ) -> Result<Response<GetBlockWithTxsResponse>, Status> {
        dbg!("get block with txs");

        todo!()
    }
}
