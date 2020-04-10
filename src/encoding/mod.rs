use std::io::{Read, Write};
use failure::bail;
use crate::Result;

pub trait Encode {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()>;
    fn encoding_length(&self) -> Result<usize>;

    fn encode(&self) -> Result<Vec<u8>> {
        let length = self.encoding_length()?;
        let mut bytes = Vec::with_capacity(length);
        self.encode_into(&mut bytes)?;
        Ok(bytes)
    }
}

pub trait Decode: Sized {
    fn decode<R: Read>(input: R) -> Result<Self>;
}

macro_rules! int_impl {
    ($type:ty, $length:expr) => {
        impl Encode for $type {
            fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
                let bytes = self.to_be_bytes();
                dest.write_all(&bytes[..])?;
                Ok(())
            }             

            fn encoding_length(&self) -> Result<usize> {
                Ok($length)
            }
        }

        impl Decode for $type {
            fn decode<R: Read>(mut input: R) -> Result<Self> {
                let mut bytes = [0; $length];
                input.read_exact(&mut bytes[..])?;
                Ok(Self::from_be_bytes(bytes))
            }
        }
    }
}

int_impl!(u8, 1);
int_impl!(u16, 2);
int_impl!(u32, 4);
int_impl!(u64, 8);
int_impl!(i8, 1);
int_impl!(i16, 2);
int_impl!(i32, 4);
int_impl!(i64, 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_u8() {
        let value = 0x12u8;
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0x12]);
        let decoded_value = u8::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);
    }

    #[test]
    fn encode_decode_u64() {
        let value = 0x1234567890u64;
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90]);
        let decoded_value = u64::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);
    }
}
