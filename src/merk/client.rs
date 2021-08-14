use crate::{
    store::{Read, KV},
    Result,
};

/// A client which decodes and verifies Merk proofs when accessing data from an
/// underlying [`Read`](../store/trait.Read.html).
pub struct Client<R: Read> {
    read: R,
}

impl<R: Read> Client<R> {
    /// Contstucts a `Client` which reads from the given store.
    pub fn new(read: R) -> Self {
        Self { read }
    }
}

// TODO: verify hash
impl<R: Read> Read for Client<R> {
    /// Gets a value for the given key, then decodes the response as a Merk
    /// proof and verifies it, returning the extracted value.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        todo!()
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abci::TendermintClient;

    #[test]
    #[ignore]
    fn get() {
        let tc = TendermintClient::new("localhost:26657").unwrap();
        let mc = Client::new(tc);
        let data = mc.get(b"count").unwrap();
        println!("{:?}", data.unwrap());
    }
}
