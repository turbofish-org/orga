use crate::{Read, Result};
use blocking::block_on;
use failure::format_err;
use tendermint_rpc::Client;

pub struct TendermintClient {
    client: Client,
}

impl TendermintClient {
    pub fn new(addr: &str) -> Result<TendermintClient> {
        Ok(TendermintClient {
            client: Client::new(
                addr.parse()
                    .map_err(|_| format_err!("Invalid Tendermint RPC address"))?,
            ),
        })
    }
}

impl Read for TendermintClient {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(Some(
            block_on(self.client.abci_query(None, key, None, false))?.value,
        ))
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
