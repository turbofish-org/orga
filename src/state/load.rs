use std::marker::PhantomData;

use crate::migrate::MigrateInto;
use crate::store::Store;
use crate::Result;

use super::State;

pub struct Loader<'a, 'b> {
    version: u8,
    field_count: u8,
    store: Store,
    bytes: &'a mut &'b [u8],
}

impl<'a, 'b> Loader<'a, 'b> {
    pub fn new(store: Store, bytes: &'a mut &'b [u8], version: u8) -> Self {
        Self {
            field_count: 0,
            version,
            store,
            bytes,
        }
    }

    pub fn maybe_load_from_prev<T, U>(&mut self) -> Result<Option<U>>
    where
        T: MigrateInto<U> + State,
    {
        let res = if !self.bytes.is_empty() && self.bytes[0] < self.version {
            let value = T::load(self.store.clone(), self.bytes)?;
            Some(value.migrate_into()?)
        } else {
            None
        };

        Ok(res)
    }

    pub fn load_child<U>(&mut self) -> Result<U>
    where
        U: State,
    {
        if self.field_count == 0 {
            *self.bytes = &self.bytes[1..];
        }
        let res = U::load(self.store.sub(&[self.field_count]), self.bytes);
        self.field_count += 1;

        res
    }

    pub fn load_child_as<T, U>(&mut self) -> Result<U>
    where
        U: From<T> + State,
        T: State,
    {
        let value = self.load_child::<T>()?;
        Ok(value.into())
    }
}
