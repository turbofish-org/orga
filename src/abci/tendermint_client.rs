use std::convert::TryInto;
use std::ops::{Deref, DerefMut};

use tendermint_rpc as tm;
use tm::Client as _;

use crate::call::Call;
use crate::client::{AsyncCall, Client};
use crate::encoding::{Decode, Encode};
use crate::merk::ProofStore;
use crate::query::Query;
use crate::state::State;
use crate::store::{Shared, Store};
use crate::Result;

pub use tm::endpoint::broadcast::tx_commit::Response as TxResponse;

pub struct TendermintClient<T: Client<TendermintAdapter<T>>> {
    state_client: T::Client,
    tm_client: tm::HttpClient,
}

impl<T: Client<TendermintAdapter<T>>> TendermintClient<T> {
    pub fn new(addr: &str) -> Result<Self> {
        let tm_client = tm::HttpClient::new(addr)?;
        let state_client = T::create_client(TendermintAdapter {
            marker: std::marker::PhantomData,
            client: tm_client.clone(),
        });
        Ok(TendermintClient {
            state_client,
            tm_client,
        })
    }
}

impl<T: Client<TendermintAdapter<T>>> Deref for TendermintClient<T> {
    type Target = T::Client;

    fn deref(&self) -> &Self::Target {
        &self.state_client
    }
}

impl<T: Client<TendermintAdapter<T>>> DerefMut for TendermintClient<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state_client
    }
}

impl<T: Client<TendermintAdapter<T>> + Query + State> TendermintClient<T> {
    pub async fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
    where
        F: Fn(&T) -> Result<R>,
    {
        let query_bytes = query.encode()?;
        let res = self
            .tm_client
            .abci_query(None, query_bytes, None, true)
            .await?;
        let root_hash = res.value[0..32].try_into()?;
        let proof_bytes = &res.value[32..];

        let map = merk::proofs::query::verify(proof_bytes, root_hash)?;
        let root_value = match map.get(&[])? {
            Some(root_value) => root_value,
            None => return Err(failure::format_err!("missing root value")),
        };
        let encoding = T::Encoding::decode(root_value)?;
        let store: Shared<ProofStore> = Shared::new(ProofStore(map));
        let state = T::create(Store::new(store.into()), encoding)?;

        // TODO: retry logic
        check(&state)
    }
}

pub struct TendermintAdapter<T> {
    marker: std::marker::PhantomData<T>,
    client: tm::HttpClient,
}

impl<T> Clone for TendermintAdapter<T> {
    fn clone(&self) -> TendermintAdapter<T> {
        TendermintAdapter {
            marker: self.marker,
            client: self.client.clone(),
        }
    }
}

unsafe impl<T> Send for TendermintAdapter<T> {}

#[async_trait::async_trait]
impl<T: Call> AsyncCall for TendermintAdapter<T>
where
    T::Call: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        let tx = call.encode()?.into();
        let fut = self.client.broadcast_tx_commit(tx);
        NoReturn(fut).await
    }
}

pub struct NoReturn<'a>(
    std::pin::Pin<Box<dyn std::future::Future<Output = tm::Result<TxResponse>> + Send + 'a>>,
);

impl<'a> std::future::Future for NoReturn<'a> {
    type Output = Result<()>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context,
    ) -> std::task::Poll<Self::Output> {
        unsafe {
            let mut_ref = self.get_unchecked_mut();
            let res = mut_ref.0.as_mut().poll(cx);
            match res {
                std::task::Poll::Ready(Ok(tx_res)) => {
                    if tx_res.check_tx.code.is_err() {
                        std::task::Poll::Ready(Err(failure::format_err!(
                            "CheckTx failed: {}",
                            tx_res.check_tx.log
                        )))
                    } else if tx_res.deliver_tx.code.is_err() {
                        std::task::Poll::Ready(Err(failure::format_err!(
                            "DeliverTx failed: {}",
                            tx_res.deliver_tx.log
                        )))
                    } else {
                        std::task::Poll::Ready(Ok(()))
                    }
                }
                std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e.into())),
                std::task::Poll::Pending => std::task::Poll::Pending,
            }
        }
    }
}
