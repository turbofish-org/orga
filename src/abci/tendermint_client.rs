use crate::{
    store::{Read, KV},
    Result,
};
use blocking::block_on;
use tendermint_rpc::{Client, HttpClient};

/// A [`store::Read`](../store/trait.Read.html) implementation which queries a
/// Tendermint node's state via RPC.
///
/// This client simply passes the query response through as bytes, so if the
/// query response is a merkle proof then another layer must be used to verify,
/// for example [`MerkClient`](../merkstore/struct.Client.html).
pub struct TendermintClient {
    client: HttpClient,
}

impl TendermintClient {
    /// Constructs a `TendermintClient` to make RPC requests to the given
    /// address (for example, "localhost:26657").
    ///
    /// This will only fail for incorrectly formatted addresses, but will still
    /// succeed even if the address is not a valid Tendermint RPC server.
    /// Instead, if the address is not a valid server, requests made through the
    /// client will fail.
    pub fn new(addr: &str) -> Result<TendermintClient> {
        Ok(TendermintClient {
            client: HttpClient::new(addr)?,
        })
    }
}

impl Read for TendermintClient {
    /// Gets a value from the store by making a raw key query to the Tendermint
    /// RPC server. The raw response bytes will be returned, which may include
    /// unverified proof bytes depending on the node's
    /// [`ABCIStore`](../trait.ABCIStore.html) implementation.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(Some(
            block_on(self.client.abci_query(None, key, None, false))?.value,
        ))
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn get() {
        let tc = TendermintClient::new("localhost:26657").unwrap();
        let data = tc.get(b"count").unwrap();
        println!("{:?}", data.unwrap()[3]);
    }
}
