use tendermint_rpc as tm;
use tm::Client as _;

use crate::call::Call;
use crate::client::AsyncCall;
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
            marker: self.marker.clone(),
            client: self.client.clone(),
        }
    }
}

impl<T> TendermintClient<T> {
    pub fn new(addr: &str) -> Result<Self> {
        Ok(TendermintClient {
            marker: std::marker::PhantomData,
            client: tm::HttpClient::new(addr)?,
        })
    }
}

use std::future::Future;
use std::pin::Pin;
impl<T: Call> AsyncCall for TendermintClient<T>
where
    T::Call: Send,
{
    type Call = T::Call;
    type Future = NoReturn;

    fn call(&mut self, call: Self::Call) -> Self::Future {
        let tx = call.encode().unwrap().into();
        let fut = self.client.broadcast_tx_commit(tx);
        let fut2: Pin<Box<dyn Future<Output = tm::Result<TxResponse>> + 'static>> =
            unsafe { std::mem::transmute(fut) };
        NoReturn(fut2)
    }
}

pub struct NoReturn(Pin<Box<dyn Future<Output = tm::Result<TxResponse>> + 'static>>);

impl Future for NoReturn {
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
