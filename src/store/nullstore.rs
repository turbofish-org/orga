use super::*;

pub struct NullStore;

impl Read for NullStore {
    fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

// TODO: should writes fail? or should this only implement Read?
impl Write for NullStore {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        Ok(())
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        Ok(())
    }
}
