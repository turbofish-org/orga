use tendermint::rpc::Client;
use crate::{Read, Result};
use failure::Error;

struct TendermintClient {
    client: Client
}

impl TendermintClient {
    fn new(addr: &str) -> Result<TendermintClient> {
        Ok(TendermintClient {  
            client: Client::new(&(addr.parse()?))?
        })
    }
}

impl Read for TendermintClient {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok((self.client.abci_query(None, key, None, false)?).value)
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

