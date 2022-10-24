use ibc_proto::cosmos::bank::v1beta1::{
    query_server::Query as BankQuery, QueryAllBalancesRequest, QueryAllBalancesResponse,
    QueryBalanceRequest, QueryBalanceResponse, QueryDenomMetadataRequest,
    QueryDenomMetadataResponse, QueryDenomOwnersRequest, QueryDenomOwnersResponse,
    QueryDenomsMetadataRequest, QueryDenomsMetadataResponse, QueryParamsRequest,
    QueryParamsResponse, QuerySpendableBalancesRequest, QuerySpendableBalancesResponse,
    QuerySupplyOfRequest, QuerySupplyOfResponse, QueryTotalSupplyRequest, QueryTotalSupplyResponse,
};

use super::Ibc;
use crate::abci::tendermint_client::TendermintAdapter;
use crate::client::{AsyncQuery, Client};
use crate::query::Query;
use std::rc::Rc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T, U> BankQuery for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
{
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
    async fn denom_owners(
        &self,
        _request: Request<QueryDenomOwnersRequest>,
    ) -> Result<Response<QueryDenomOwnersResponse>, Status> {
        unimplemented!()
    }
}
