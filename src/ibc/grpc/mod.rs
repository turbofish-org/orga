use super::Ibc;
use crate::client::{AsyncCall, AsyncQuery, Call, Client};
use crate::plugins::ibc::{IbcAdapter, IbcPlugin};
use crate::query::Query;
use crate::state::State;
use cosmos_sdk_proto::cosmos::auth::v1beta1::query_server::QueryServer as AuthQueryServer;
use cosmos_sdk_proto::cosmos::base::tendermint::v1beta1::service_server::ServiceServer as HealthServer;
use cosmos_sdk_proto::cosmos::tx::v1beta1::service_server::ServiceServer as TxServer;
use cosmos_sdk_proto::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
use tonic::transport::Server;

mod auth;
mod client;
mod health;
mod tx;

type AppClient<T> = <Ibc as Client<T>>::Client;

#[derive(Clone)]
struct GrpcServer<T>
where
    T: Clone + Send + Sync,
{
    ibc: AppClient<T>,
}

impl<T> GrpcServer<T>
where
    T: Clone + Send + Sync,
{
    pub fn new(ibc: AppClient<T>) -> Self {
        Self { ibc }
    }
}

pub async fn start_grpc<T>(ibc: AppClient<T>)
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: AsyncQuery<Response = Ibc>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
{
    println!("started grpc server");
    let server = GrpcServer::new(ibc);
    Server::builder()
        .add_service(TxServer::new(server.clone()))
        .add_service(ClientQueryServer::new(server.clone()))
        .add_service(HealthServer::new(server.clone()))
        .add_service(AuthQueryServer::new(server.clone()))
        .serve("127.0.0.1:9001".parse().unwrap())
        .await
        .unwrap();

    // let res = ibc.deliver_message().await.unwrap();
}
