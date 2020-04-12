use crate::{Read, Result};

pub struct Client<R: Read> {
    read: R
}

impl<R: Read> Client<R> {
    pub fn new(read: R) -> Self {
        Self { read }  
    }
}

// TODO: verify hash
impl<R: Read> Read for Client<R> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let data = self.read.get(key)?.unwrap();
        let mut hash = [0; 20];
        hash.copy_from_slice(&data[0..20]);
        let proof = &data[20..];
        Ok(merk::verify_proof(proof, &[key.to_vec()], hash)?[0].clone())
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
