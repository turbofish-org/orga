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

    pub fn attach_child_with_relative_prefix<U>(
        mut self,
        value: &mut U,
        prefix: &[u8],
    ) -> Result<Self>
    where
        U: State,
    {
        let substore = self.store.sub(prefix);
        value.attach(substore)?;
        self.field_count += 1;

        Ok(self)
    }

    pub fn attach_child_with_absolute_prefix<U>(
        mut self,
        value: &mut U,
        prefix: Vec<u8>,
    ) -> Result<Self>
    where
        U: State,
    {
        let substore = self.store.with_prefix(prefix);
        value.attach(substore)?;
        self.field_count += 1;

        Ok(self)
    }

    pub fn attach_child_as<T, U>(mut self, _value: U) -> Result<Self> {
        self.field_count += 1;
        Ok(self)
    }

    pub fn attach_skipped_child<T>(self, _value: T) -> Result<Self> {
        Ok(self)
    }

    pub fn attach_transparent_child<T: State>(self, value: &mut T) -> Result<Self> {
        value.attach(self.store.clone())?;
        Ok(self)
    }
}
