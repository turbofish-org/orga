use super::*;

pub struct NullStore;

impl Read for NullStore {
    fn get<K: AsRef<[u8]>>(&self, _key: K) -> Result<Vec<u8>> {
        Err(Error::from(ErrorKind::NotFound).into())
    }
}

impl Write for NullStore {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, _key: K) -> Result<()> {
        Ok(())
    }
}
