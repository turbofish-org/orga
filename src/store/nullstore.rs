use super::*;

/// A dummy implementation of `Store` which is always empty.
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
