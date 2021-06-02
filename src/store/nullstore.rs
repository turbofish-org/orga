use super::*;

#[derive(Default)]
pub struct NullStore;

impl Read for NullStore {
    #[inline]
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    #[inline]
    fn get_next(&self, _: &[u8]) -> Result<Option<KV>> {
        Ok(None)
    }

    #[inline]
    fn get_prev(&self, _: &[u8]) -> Result<Option<KV>> {
        Ok(None)
    }
}
