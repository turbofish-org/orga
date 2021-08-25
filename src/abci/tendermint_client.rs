use blocking::block_on;
use tendermint_rpc as tm;
use tm::Client as _;
use failure::bail;

use crate::encoding::Encode;
use crate::call::Call;
use crate::client::Client;
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

impl<T: Call> Client<T> for TendermintClient<T> {
    type CallRes = TxResponse;

    // fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
    //   where F: Fn(T::Res) -> Result<R>
    // {
    //   todo!()
    // }

    fn call(&mut self, call: T::Call) -> Result<Self::CallRes> {
        let tx = call.encode()?.into();
        let res = block_on(self.client.broadcast_tx_commit(tx))?;

        if res.check_tx.code.is_err() {
            bail!("Call failed on checkTx: {:?}", res.check_tx);
        }

        if res.deliver_tx.code.is_err() {
            bail!("Call failed on deliverTx: {:?}", res.deliver_tx);
        }

        Ok(res)
    }
}
