//! Null / empty stores.
use super::*;
use crate::Error as OrgaError;

/// An implementation of `Read` which is always empty. Useful for tests, or as
/// the "backing store" for a `BufStore` in order to create a temporary store
/// which does not persist data (`MapStore`).
#[derive(Default, Clone)]
pub struct Empty;

impl Read for Empty {
    #[inline]
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    #[inline]
    fn get_next(&self, _: &[u8]) -> Result<Option<KV>> {
        Ok(None)
    }

    #[inline]
    fn get_prev(&self, _key: Option<&[u8]>) -> Result<Option<KV>> {
        Ok(None)
    }
}

impl Write for Empty {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        unimplemented!()
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        unimplemented!()
    }
}

/// A store for which reads always produce an [Error::GetUnknown] store error.
#[derive(Default, Clone)]
pub struct Unknown;

impl Read for Unknown {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Err(OrgaError::StoreErr(Error::GetUnknown(key.to_vec())))
    }

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        Err(OrgaError::StoreErr(Error::GetUnknown(key.to_vec())))
    }

    #[inline]
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        Err(OrgaError::StoreErr(Error::GetUnknown(
            key.unwrap().to_vec(),
        )))
    }
}

impl Write for Unknown {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        // TODO: WriteUnknown error
        unimplemented!()
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        // TODO: WriteUnknown error
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn get() {
        let store = Empty;
        assert_eq!(store.get(&[1]).unwrap(), None)
    }

    #[test]
    fn get_next() {
        let store = Empty;
        assert_eq!(store.get_next(&[1]).unwrap(), None)
    }

    #[test]
    fn get_prev() {
        let store = Empty;
        assert_eq!(store.get_prev(Some(&[1])).unwrap(), None)
    }
}
