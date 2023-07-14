use std::any::type_name;

use crate::compat_mode;
use crate::migrate::Migrate;
use crate::store::Store;
use crate::{Error, Result};

pub struct Migration<'a, 'b> {
    version: u8,
    field_count: u8,
    src: Store,
    dest: Store,
    bytes: &'a mut &'b [u8],
}

impl<'a, 'b> Migration<'a, 'b> {
    pub fn new(src: Store, dest: Store, bytes: &'a mut &'b [u8], version: u8) -> Self {
        Self {
            field_count: 0,
            version,
            src,
            dest,
            bytes,
        }
    }

    // TODO: parameterize type with T so we don't have to pass it here
    pub fn migrate_child<T, U>(&mut self) -> Result<U>
    where
        U: Migrate,
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

        let res = U::migrate(
            self.src.sub(&[self.field_count]),
            self.dest.sub(&[self.field_count]),
            self.bytes,
        );

        self.field_count += 1;

        res
    }

    pub fn migrate_child_as<T, U>(&mut self) -> Result<U>
    where
        U: From<T>,
        T: Migrate,
    {
        let value = self.migrate_child::<T, T>()?;
        Ok(value.into())
    }

    pub fn migrate_skipped_child<T: Default>(&mut self) -> Result<T> {
        Ok(T::default())
    }

    pub fn migrate_transparent_child_inner<T: Migrate>(&mut self) -> Result<T> {
        if !compat_mode() {
            *self.bytes = &self.bytes[1..];
        }
        T::migrate(self.src.clone(), self.dest.clone(), self.bytes)
    }

    pub fn migrate_transparent_child_other<T: Default>(&mut self) -> Result<T> {
        Ok(T::default())
    }
}
