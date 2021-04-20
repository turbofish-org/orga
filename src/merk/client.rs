use crate::{store::Read, Result};

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
        let data = self.read.get(key)?.unwrap();
        let mut hash = [0; 20];
        hash.copy_from_slice(&data[0..20]);
        let proof = &data[20..];
        Ok(merk::verify_query(proof, &[key.to_vec()], hash)?[0].clone())
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
