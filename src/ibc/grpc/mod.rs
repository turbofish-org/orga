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

use crate::error::Result;
use ibc_proto::ibc::core::client::v1::Height as RawHeight;
use std::sync::{Arc, Mutex};
use tonic::transport::Server;

use crate::abci::tendermint_client::TendermintAdapter;
mod auth;
mod bank;
mod channel;
mod client;
mod connection;
mod health;
mod staking;
mod tx;

type AppClient<T> = <Ibc as Client<T>>::Client;

type IbcProvider<T, U: Client<TendermintAdapter<U>>> =
    &'static (dyn Fn(U::Client) -> AppClient<T> + Send + Sync);

struct GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    U: Client<TendermintAdapter<U>> + 'static,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send + 'static,
{
    ibc: AppClient<T>,
    tm_client: TendermintClient<U>,
    ibc_provider: IbcProvider<T, U>,
}

impl<T, U> Clone for GrpcServer<T, U>
where
    T: Clone + Send + Sync,
    U: Client<TendermintAdapter<U>> + 'static,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send + 'static,
{
    fn clone(&self) -> Self {
        GrpcServer {
            ibc_provider: self.ibc_provider,
            ibc: self.ibc.clone(),
            tm_client: self.tm_client.clone(),
        }
    }
}

impl<T, U> GrpcServer<T, U>
where
    T: Clone + Send + Sync + 'static,
    U: Client<TendermintAdapter<U>> + 'static,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send + 'static,
{
    pub fn new(
        tm_client: TendermintClient<U>,
        ibc: AppClient<T>,
        ibc_provider: IbcProvider<T, U>,
    ) -> Self {
        Self {
            tm_client,
            ibc,
            ibc_provider,
        }
    }

    async fn height(&self) -> u64 {
        // TODO: remove this function, get height from query responses
        use tendermint_rpc::Client;
        let client = tendermint_rpc::HttpClient::new("http://localhost:26357").unwrap();
        let status = client.status().await.unwrap();
        status.sync_info.latest_block_height.into()
    }

    async fn ibc_with_height<
        R,
        F: FnOnce(U::Client) -> X,
        X: std::future::Future<Output = Result<R>>,
    >(
        &self,
        f: F,
    ) -> Result<(R, u64)> {
        let response = self.tm_client.with_response(f).await?;

        Ok((response.0, response.1.height.into()))
    }
}

pub async fn start_grpc<T, U>(
    tm_client: TendermintClient<U>,
    ibc: AppClient<T>,
    ibc_provider: IbcProvider<T, U>,
) where
    T: Clone + Send + Sync + 'static,
    // T: AsyncCall<Call = <Ibc as Call>::Call>,
    T: AsyncQuery,
    T: for<'a> AsyncQuery<Response<'a> = Rc<Ibc>>,
    T: AsyncQuery<Query = <Ibc as Query>::Query>,
    U: Client<TendermintAdapter<U>> + 'static,
    <U as Client<TendermintAdapter<U>>>::Client: Sync + Send + 'static,
{
    println!("started grpc server");
    let server = GrpcServer::new(tm_client, ibc, ibc_provider);
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
