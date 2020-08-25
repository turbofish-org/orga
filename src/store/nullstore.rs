use super::*;

#[derive(Default)]
pub struct NullStore;

impl Read for NullStore {
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl Iter for NullStore {
    type Iter<'a> = NullIter<'a>;

    fn iter_from(&self, _start: &[u8]) -> NullIter {
        NullIter(std::marker::PhantomData)
    }
}

pub struct NullIter<'a>(std::marker::PhantomData<&'a ()>);

impl<'a> Iterator for NullIter<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
