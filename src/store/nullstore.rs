use super::*;

pub struct NullStore;

impl Read for NullStore {
    fn get<K: AsRef<[u8]>>(&self, _key: K) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

// TODO: should writes fail? or should this only implement Read?
impl Write for NullStore {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, _key: K) -> Result<()> {
        Ok(())
    }
}
