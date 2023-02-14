use crate::compat_mode;
use crate::encoding::{Decode, Error, Result};
use crate::migrate::MigrateInto;
use std::io::Read;

pub struct Decoder<R> {
    field_count: u8,
    version: u8,
    bytes: R,
}

impl<R: Read> Decoder<R> {
    pub fn new(bytes: R, version: u8) -> Self {
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
        let value = if compat_mode() {
            Some(T::decode(&mut self.bytes)?)
        } else {
            let version_byte = u8::decode(&mut self.bytes)?;
            if version_byte < self.version {
                let byte_prefix = vec![version_byte];
                let mut inp = byte_prefix.chain(&mut self.bytes);
                Some(T::decode(&mut inp)?)
            } else {
                None
            }
        }
        .map(|v| v.migrate_into().map_err(|_| Error::UnexpectedByte(0)))
        .transpose()?;

        Ok(value)
    }

    pub fn decode_child<U>(&mut self) -> Result<U>
    where
        U: Decode,
    {
        if self.field_count == 0 && !compat_mode() && self.version == 0 {
            let _version_byte = u8::decode(&mut self.bytes)?;
        }
        let res = U::decode(&mut self.bytes);
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
