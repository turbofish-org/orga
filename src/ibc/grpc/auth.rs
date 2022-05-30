use cosmos_sdk_proto::cosmos::auth::v1beta1::query_server::Query as AuthQuery;
use cosmos_sdk_proto::cosmos::auth::v1beta1::BaseAccount;
use cosmos_sdk_proto::cosmos::auth::v1beta1::{
    QueryAccountRequest, QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
    QueryParamsRequest as AuthQueryParamsRequest, QueryParamsResponse as AuthQueryParamsResponse,
};
use prost::Message;
use prost_types::Any;

use super::Ibc;
use crate::client::{AsyncCall, AsyncQuery, Call};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl<T> AuthQuery for super::GrpcServer<T>
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: AsyncQuery<Response = Ibc>,
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
        println!("auth.account()");
        dbg!(&request);

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
        // unimplemented!()
        // debug!("Got auth account request");

        // let mut account = self.account.write().unwrap();
        // let mut buf = Vec::new();
        // account.encode(&mut buf).unwrap(); // safety - cannot fail since buf is a vector
        // account.sequence += 1;

        // Ok(Response::new(QueryAccountResponse {
        //     account: Some(Any {
        //         type_url: "/cosmos.auth.v1beta1.BaseAccount".to_string(),
        //         value: buf,
        //     }),
        // }))
    }

    async fn params(
        &self,
        _request: Request<AuthQueryParamsRequest>,
    ) -> Result<Response<AuthQueryParamsResponse>, Status> {
        unimplemented!()
    }
}
