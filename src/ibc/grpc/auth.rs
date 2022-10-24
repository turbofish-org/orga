use super::Ibc;
use crate::abci::tendermint_client::TendermintAdapter;
use crate::client::AsyncQuery;
use crate::client::Client;
use ibc_proto::cosmos::auth::v1beta1::AddressBytesToStringRequest;
use ibc_proto::cosmos::auth::v1beta1::AddressBytesToStringResponse;
use ibc_proto::cosmos::auth::v1beta1::AddressStringToBytesRequest;
use ibc_proto::cosmos::auth::v1beta1::AddressStringToBytesResponse;
use ibc_proto::cosmos::auth::v1beta1::BaseAccount;
use ibc_proto::cosmos::auth::v1beta1::Bech32PrefixRequest;
use ibc_proto::cosmos::auth::v1beta1::Bech32PrefixResponse;
use ibc_proto::cosmos::auth::v1beta1::QueryAccountAddressByIdRequest;
use ibc_proto::cosmos::auth::v1beta1::QueryAccountAddressByIdResponse;
use ibc_proto::cosmos::auth::v1beta1::QueryModuleAccountsRequest;
use ibc_proto::cosmos::auth::v1beta1::QueryModuleAccountsResponse;
use ibc_proto::cosmos::auth::v1beta1::{
    query_server::Query as AuthQuery, QueryAccountRequest, QueryAccountResponse,
    QueryAccountsRequest, QueryAccountsResponse, QueryParamsRequest, QueryParamsResponse,
};
use ibc_proto::google::protobuf::Any;
use prost::Message;
use std::rc::Rc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T, U> AuthQuery for super::GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    U: Client<TendermintAdapter<U>>,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send,
{
    async fn accounts(
        &self,
        _request: Request<QueryAccountsRequest>,
    ) -> Result<Response<QueryAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn account(
        &self,
        request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        let address = request.get_ref().address.clone();
        let account = BaseAccount {
            address,
            ..Default::default()
        };
        Ok(Response::new(QueryAccountResponse {
            account: Some(Any {
                type_url: "/cosmos.auth.v1beta1.BaseAccount".to_string(),
                value: account.encode_to_vec(),
            }),
        }))
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn module_accounts(
        &self,
        _request: Request<QueryModuleAccountsRequest>,
    ) -> Result<Response<QueryModuleAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn account_address_by_id(
        &self,
        _request: Request<QueryAccountAddressByIdRequest>,
    ) -> Result<Response<QueryAccountAddressByIdResponse>, Status> {
        unimplemented!()
    }

    async fn bech32_prefix(
        &self,
        _request: Request<Bech32PrefixRequest>,
    ) -> Result<Response<Bech32PrefixResponse>, Status> {
        unimplemented!()
    }

    async fn address_string_to_bytes(
        &self,
        _request: Request<AddressStringToBytesRequest>,
    ) -> Result<Response<AddressStringToBytesResponse>, Status> {
        unimplemented!()
    }

    async fn address_bytes_to_string(
        &self,
        _request: Request<AddressBytesToStringRequest>,
    ) -> Result<Response<AddressBytesToStringResponse>, Status> {
        unimplemented!()
    }
}
