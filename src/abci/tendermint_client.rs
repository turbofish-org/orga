use tendermint_rpc as tm;
use tm::Client as _;

use crate::call::Call;
use crate::client::{AsyncCall, Client};
use crate::encoding::Encode;
use crate::Result;

pub use tm::endpoint::broadcast::tx_commit::Response as TxResponse;

pub struct TendermintClient<T> {
    marker: std::marker::PhantomData<T>,
    client: tm::HttpClient,
}

impl<T> Clone for TendermintClient<T> {
    fn clone(&self) -> TendermintClient<T> {
        TendermintClient {
            marker: self.marker,
            client: self.client.clone(),
        }
    }
}

impl<T: Client<Self>> TendermintClient<T> {
    pub fn new(addr: &str) -> Result<T::Client> {
        Ok(T::create_client(TendermintClient {
            marker: std::marker::PhantomData,
            client: tm::HttpClient::new(addr)?,
        }))
    }
}

unsafe impl<T> Send for TendermintClient<T> {}

use std::future::Future;
use std::pin::Pin;
impl<T: Call> AsyncCall for TendermintClient<T>
where
    T::Call: Send,
{
    type Call = T::Call;
    type Future<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    fn call(&mut self, call: Self::Call) -> Self::Future<'_> {
        Box::pin(async move {
            let tx = call.encode()?.into();
            let fut = self.client.broadcast_tx_commit(tx);
            NoReturn(fut).await
        })
    }
}

pub struct NoReturn<'a>(Pin<Box<dyn Future<Output = tm::Result<TxResponse>> + Send + 'a>>);

impl<'a> Future for NoReturn<'a> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context) -> std::task::Poll<Self::Output> {
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
