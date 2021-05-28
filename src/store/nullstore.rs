use super::*;

#[derive(Default)]
pub struct NullStore;

impl Read for NullStore {
    fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl Iter for NullStore {
    fn iter_from(&self, _: &[u8]) -> EntryIter {
        Box::new(NullIter)
    }
}

pub struct NullIter;

impl Iterator for NullIter {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
