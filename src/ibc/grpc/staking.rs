use ibc_proto::{
    cosmos::staking::v1beta1::{
        query_server::Query as StakingQuery, Params, QueryDelegationRequest,
        QueryDelegationResponse, QueryDelegatorDelegationsRequest,
        QueryDelegatorDelegationsResponse, QueryDelegatorUnbondingDelegationsRequest,
        QueryDelegatorUnbondingDelegationsResponse, QueryDelegatorValidatorRequest,
        QueryDelegatorValidatorResponse, QueryDelegatorValidatorsRequest,
        QueryDelegatorValidatorsResponse, QueryHistoricalInfoRequest, QueryHistoricalInfoResponse,
        QueryParamsRequest, QueryParamsResponse, QueryPoolRequest, QueryPoolResponse,
        QueryRedelegationsRequest, QueryRedelegationsResponse, QueryUnbondingDelegationRequest,
        QueryUnbondingDelegationResponse, QueryValidatorDelegationsRequest,
        QueryValidatorDelegationsResponse, QueryValidatorRequest, QueryValidatorResponse,
        QueryValidatorUnbondingDelegationsRequest, QueryValidatorUnbondingDelegationsResponse,
        QueryValidatorsRequest, QueryValidatorsResponse,
    },
    google::protobuf::Duration,
};
use tonic::{Request, Response, Status};

use super::Ibc;
use crate::abci::tendermint_client::TendermintAdapter;
use crate::client::{AsyncQuery, Client};
use crate::query::Query;
use std::rc::Rc;

#[tonic::async_trait]
impl<T, U> StakingQuery for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
{
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
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        Ok(Response::new(QueryParamsResponse {
            params: Some(Params {
                unbonding_time: Some(Duration {
                    seconds: 3 * 7 * 24 * 60 * 60,
                    nanos: 0,
                }),
                historical_entries: 1, // just to satisfy hermes's health-check
                ..Params::default()
            }),
        }))
    }
}
