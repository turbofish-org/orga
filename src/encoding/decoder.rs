use crate::compat_mode;
use crate::encoding::{Decode, Result};
use std::io::Read;

/// A helper for decoding versioned [Decode] types.
pub struct Decoder<R> {
    field_count: u8,
    version: u8,
    bytes: R,
}

impl<R: Read> Decoder<R> {
    /// Create a new decoder.
    pub fn new(bytes: R, version: u8) -> Self {
        Self {
            version,
            bytes,
            field_count: 0,
        }
    }

    /// Decode the next child from the reader, checking the version byte if this
    /// is the first field in the parent.
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

    /// Decode the next child from the reader, converting it to the given type.
    pub fn decode_child_as<T, U>(&mut self) -> Result<U>
    where
        U: From<T>,
        T: Decode,
    {
        let value = self.decode_child::<T>()?;
        Ok(value.into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn simple() {
        let bytes = [0, 1];
        let mut bytes = &bytes[..];
        let mut decoder = super::Decoder::new(&mut bytes, 0);
        assert_eq!(decoder.decode_child::<u8>().unwrap(), 1);
    }

    #[test]
    fn higher_version() {
        let bytes = [10, 1];
        let mut bytes = &bytes[..];
        let mut decoder = super::Decoder::new(&mut bytes, 10);
        assert_eq!(decoder.decode_child::<u8>().unwrap(), 1);
    }

    #[test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value: UnexpectedByte(0)")]
    fn incorrect_version() {
        let bytes = [0, 1];
        let mut bytes = &bytes[..];
        let mut decoder = super::Decoder::new(&mut bytes, 1);
        decoder.decode_child::<u8>().unwrap();
    }
}
