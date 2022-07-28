use std::rc::Rc;

use super::Ibc;
use crate::abci::TendermintClient;
use crate::client::{AsyncCall, AsyncQuery, Call, Client};
use crate::plugins::ibc::{IbcAdapter, IbcPlugin};
use crate::query::Query;
use crate::state::State;
use ibc_proto::cosmos::auth::v1beta1::query_server::QueryServer as AuthQueryServer;
use ibc_proto::cosmos::bank::v1beta1::query_server::QueryServer as BankQueryServer;
use ibc_proto::cosmos::base::tendermint::v1beta1::service_server::ServiceServer as HealthServer;
use ibc_proto::cosmos::staking::v1beta1::query_server::QueryServer as StakingQueryServer;
use ibc_proto::cosmos::tx::v1beta1::service_server::ServiceServer as TxServer;
use ibc_proto::ibc::core::channel::v1::query_server::QueryServer as ChannelQueryServer;
use ibc_proto::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
use ibc_proto::ibc::core::connection::v1::query_server::QueryServer as ConnectionQueryServer;

use tonic::transport::Server;

mod auth;
mod bank;
mod channel;
mod client;
mod connection;
mod health;
mod staking;
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

    async fn height(&self) -> u64 {
        // TODO: remove this function, get height from query responses
        use tendermint_rpc::Client;
        let client = tendermint_rpc::HttpClient::new("http://localhost:26357").unwrap();
        let status = client.status().await.unwrap();
        status.sync_info.latest_block_height.into()
    }
}

pub async fn start_grpc<T>(ibc: AppClient<T>)
where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
{
    println!("started grpc server");
    let server = GrpcServer::new(ibc);
    Server::builder()
        .add_service(TxServer::new(server.clone()))
        .add_service(ClientQueryServer::new(server.clone()))
        .add_service(ConnectionQueryServer::new(server.clone()))
        .add_service(ChannelQueryServer::new(server.clone()))
        .add_service(HealthServer::new(server.clone()))
        .add_service(AuthQueryServer::new(server.clone()))
        .add_service(BankQueryServer::new(server.clone()))
        .add_service(StakingQueryServer::new(server.clone()))
        .serve("127.0.0.1:9001".parse().unwrap())
        .await
        .unwrap();

    // let res = ibc.deliver_message().await.unwrap();
}

impl From<orga::Error> for tonic::Status {
    fn from(err: orga::Error) -> Self {
        tonic::Status::aborted(err.to_string())
    }
}
