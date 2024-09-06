use super::State;
use crate::store::Store;
use crate::Result;

/// A helper for attaching children in [State] implementations, used by the
/// derive macro.
pub struct Attacher {
    store: Store,
    field_count: u8,
}

impl Attacher {
    /// Create a new [Attacher] for the given store.
    pub fn new(store: Store) -> Self {
        Self {
            store,
            field_count: 0,
        }
    }

    /// Attach a child to the store.
    pub fn attach_child<U>(mut self, value: &mut U) -> Result<Self>
    where
        U: State,
    {
        value.attach(self.store.sub(&[self.field_count]))?;
        self.field_count += 1;

        Ok(self)
    }

    /// Attach a child to the store with a relative prefix.
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

    /// Attach a child to the store with an absolute prefix.
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

    /// Simply increments the field count at the moment, does not actually
    /// attach.
    pub fn attach_child_as<T, U>(mut self, _value: U) -> Result<Self> {
        self.field_count += 1;
        Ok(self)
    }

    /// No-op for skipped children.
    pub fn attach_skipped_child<T>(self, _value: T) -> Result<Self> {
        Ok(self)
    }

    /// Attach the child directly to the parent's store.
    pub fn attach_transparent_child<T: State>(self, value: &mut T) -> Result<Self> {
        value.attach(self.store.clone())?;
        Ok(self)
    }
}
