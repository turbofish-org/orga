use super::State;
use crate::store::Store;
use crate::Result;

pub struct Attacher {
    store: Store,
    field_count: u8,
}

impl Attacher {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            field_count: 0,
        }
    }

    pub fn attach_child<U>(mut self, value: &mut U) -> Result<Self>
    where
        U: State,
    {
        value.attach(self.store.sub(&[self.field_count]))?;
        self.field_count += 1;

        Ok(self)
    }
}