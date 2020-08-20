use super::*;

/// A dummy implementation of `Read` which is always empty.
pub struct NullStore;

impl Read for NullStore {
    fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl<'a> Iter<'a> for NullStore {
    type Iter = NullIter<'a>;

    fn iter_from(&'a self, _start: &[u8]) -> NullIter {
        NullIter(std::marker::PhantomData)
    }
}

pub struct NullIter<'a>(std::marker::PhantomData<&'a ()>);

impl<'a> Iterator for NullIter<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
