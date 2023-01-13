use crate::encoding::{Decode, Error, Result};
use crate::migrate::MigrateInto;

pub struct Decoder<'a, 'b> {
    field_count: u8,
    version: u8,
    bytes: &'a mut &'b [u8],
}

impl<'a, 'b> Decoder<'a, 'b> {
    pub fn new(bytes: &'a mut &'b [u8], version: u8) -> Self {
        Self {
            version,
            bytes,
            field_count: 0,
        }
    }

    pub fn maybe_decode_from_prev<T, U>(&mut self) -> Result<Option<U>>
    where
        T: MigrateInto<U> + Decode,
    {
        let res = if !self.bytes.is_empty() && self.bytes[0] < self.version {
            let value = T::decode(self.bytes)?;
            Some(value.migrate_into().map_err(|_| Error::UnexpectedByte(0))?)
        } else {
            None
        };

        Ok(res)
    }

    pub fn decode_child<U>(&mut self) -> Result<U>
    where
        U: Decode,
    {
        if self.field_count == 0 {
            *self.bytes = &self.bytes[1..];
        }
        let res = U::decode(self.bytes);
        self.field_count += 1;

        res
    }

    pub fn decode_child_as<T, U>(&mut self) -> Result<U>
    where
        U: From<T>,
        T: Decode,
    {
        let value = self.decode_child::<T>()?;
        Ok(value.into())
    }
}
