use crate::compat_mode;
use crate::encoding::{Decode, Result};
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

    pub fn decode_child<U>(&mut self) -> Result<U>
    where
        U: Decode,
    {
        if self.field_count == 0 && !compat_mode() {
            let version_byte = u8::decode(&mut self.bytes)?;
            if version_byte != self.version {
                return Err(ed::Error::UnexpectedByte(version_byte));
            }
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
