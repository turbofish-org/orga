use std::convert::TryInto;

use cosmos_sdk_proto::cosmos::tx::v1beta1::service_server::Service as TxService;
use cosmrs::Tx;

use cosmos_sdk_proto::cosmos::tx::v1beta1::{
    BroadcastTxRequest, BroadcastTxResponse, GetBlockWithTxsRequest, GetBlockWithTxsResponse,
    GetTxRequest, GetTxResponse, GetTxsEventRequest, GetTxsEventResponse, SimulateRequest,
    SimulateResponse,
};
use ibc::core::ics26_routing::msgs::Ics26Envelope;

use super::Ibc;
use crate::client::{AsyncCall, AsyncQuery, Call};
use std::convert::TryFrom;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T> TxService for super::GrpcServer<T>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Ibc>,
{
    async fn simulate(
        &self,
        request: Request<SimulateRequest>,
    ) -> Result<Response<SimulateResponse>, Status> {
        let tx_bytes = request.get_ref().tx_bytes.as_slice();
        let tx = Tx::from_bytes(tx_bytes).unwrap();

        let msg = tx.body.messages[0].clone();
        let msg = ibc_proto::google::protobuf::Any {
            type_url: msg.type_url,
            value: msg.value,
        };
        // try making ics26 envelope
        let _envelope = Ics26Envelope::try_from(msg).unwrap();

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
