use std::any::type_name;

use crate::compat_mode;
use crate::store::Store;
use crate::{Error, Result};

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

    // TODO: parameterize type with T so we don't have to pass it here
    pub fn load_child<T, U>(&mut self) -> Result<U>
    where
        U: State,
    {
        if !compat_mode() && self.field_count == 0 {
            if self.bytes.is_empty() {
                return Err(Error::State("Unexpected EOF".to_string()));
            }

            if self.bytes[0] != self.version {
                return Err(Error::State(format!(
                    "Expected version {}, got {} for {}",
                    self.version,
                    self.bytes[0],
                    type_name::<T>()
                )));
            }
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
        let value = self.load_child::<T, T>()?;
        Ok(value.into())
    }

    pub fn load_skipped_child<T: Default>(&mut self) -> Result<T> {
        Ok(T::default())
    }

    pub fn load_transparent_child_inner<T: State>(&mut self) -> Result<T> {
        if !compat_mode() {
            *self.bytes = &self.bytes[1..];
        }
        T::load(self.store.clone(), self.bytes)
    }

    pub fn load_transparent_child_other<T: Default>(&mut self) -> Result<T> {
        Ok(T::default())
    }
}
