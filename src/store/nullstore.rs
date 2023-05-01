use super::*;

/// An implementation of `Read` which is always empty. Useful for tests, or as
/// the "backing store" for a `BufStore` in order to create a temporary store
/// which does not persist data (`MapStore`).
#[derive(Default, Clone)]
pub struct EmptyStore;

impl Read for EmptyStore {
    #[inline]
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    #[inline]
    fn get_next(&self, _: &[u8]) -> Result<Option<KV>> {
        Ok(None)
    }
}

impl Write for EmptyStore {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        unimplemented!()
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        unimplemented!()
    }
}

#[derive(Default, Clone)]
pub struct NullStore;

impl Read for NullStore {
    #[inline]
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
        Err(Error::Store("Store is null".to_string()))
    }

    #[inline]
    fn get_next(&self, _: &[u8]) -> Result<Option<KV>> {
        Err(Error::Store("Store is null".to_string()))
    }
}

impl Write for NullStore {
    fn put(&mut self, _key: Vec<u8>, _value: Vec<u8>) -> Result<()> {
        unimplemented!()
    }

    fn delete(&mut self, _key: &[u8]) -> Result<()> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn get() {
        let store = EmptyStore;
        assert_eq!(store.get(&[1]).unwrap(), None)
    }

    #[test]
    fn get_next() {
        let store = EmptyStore;
        assert_eq!(store.get_next(&[1]).unwrap(), None)
    }
}
