use super::*;

/// An implementation of `Read` which is always empty. Useful for tests, or as
/// the "backing store" for a `BufStore` in order to create a temporary store
/// which does not persist data (`MapStore`).
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn get() {
        let store = NullStore;
        assert_eq!(store.get(&[1]).unwrap(), None)
    }

    #[test]
    fn get_next() {
        let store = NullStore;
        assert_eq!(store.get_next(&[1]).unwrap(), None)
    }
}
