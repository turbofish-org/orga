use ibc::bigint::U256;
use ibc_proto::{
    cosmos::{
        bank::v1beta1::{
            query_server::{Query as BankQuery, QueryServer},
            QueryAllBalancesRequest, QueryAllBalancesResponse, QueryBalanceRequest,
            QueryBalanceResponse, QueryDenomMetadataRequest, QueryDenomMetadataResponse,
            QueryDenomsMetadataRequest, QueryDenomsMetadataResponse, QueryParamsRequest,
            QueryParamsResponse, QuerySpendableBalancesRequest, QuerySpendableBalancesResponse,
            QuerySupplyOfRequest, QuerySupplyOfResponse, QueryTotalSupplyRequest,
            QueryTotalSupplyResponse,
        },
        base::v1beta1::Coin as RawCoin,
    },
    google::protobuf::Any,
};

use super::Ibc;
use crate::client::{AsyncCall, AsyncQuery, Call};
use crate::query::Query;
use std::rc::Rc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T> BankQuery for super::GrpcServer<T>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
{
    async fn balance(
        &self,
        request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        Ok(Response::new(QueryBalanceResponse {
            balance: Some(RawCoin {
                denom: "simp".to_string(),
                amount: "1000".to_string(),
            }),
        }))
    }

    async fn all_balances(
        &self,
        _request: Request<QueryAllBalancesRequest>,
    ) -> Result<Response<QueryAllBalancesResponse>, Status> {
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

    async fn spendable_balances(
        &self,
        _request: Request<QuerySpendableBalancesRequest>,
    ) -> Result<Response<QuerySpendableBalancesResponse>, Status> {
        unimplemented!()
    }
}