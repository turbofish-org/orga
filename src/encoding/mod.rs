use std::io::{Read, Write};
use failure::{bail, format_err};
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
int_impl!(u128, 16);
int_impl!(i8, 1);
int_impl!(i16, 2);
int_impl!(i32, 4);
int_impl!(i64, 8);
int_impl!(i128, 16);

impl<T: Encode> Encode for Option<T> {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
        match self {
            None => {
                dest.write_all(&[0])
                    .map_err(|err| format_err!("{}", err))
            },
            Some(value) => {
                dest.write_all(&[1])
                    .map_err(|err| format_err!("{}", err))?;
                value.encode_into(dest)
            }
        }
    }

    fn encoding_length(&self) -> Result<usize> {
        match self {
            None => Ok(1),
            Some(value) => Ok(1 + value.encoding_length()?)
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode<R: Read>(mut input: R) -> Result<Self> {
        let mut byte = [0; 1];
        input.read_exact(&mut byte[..])?;

        let value = match byte[0] {
            0 => None,
            1 => Some(T::decode(input)?),
            byte => bail!("Unexpected byte {}", byte)
        };

        Ok(value)
    }
}

// impl<T: Encode> Encode for () {
//     fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
//         Ok(())
//     }

//     fn encoding_length(&self) -> Result<usize> {
//         Ok(0)
//     }
// }

// impl<T: Decode> Decode for () {
//     fn decode<R: Read>(mut input: R) -> Result<Self> {
//         Ok(())
//     }
// }

macro_rules! tuple_impl {
    ($( $type:ident ),*) => {
        impl<$($type: Encode),*> Encode for ($($type),*) {
            #[allow(non_snake_case, unused_mut, unused_variables)]
            fn encode_into<W: Write>(&self, mut dest: &mut W) -> Result<()> {
                let ($($type),*) = self;
                $($type.encode_into(&mut dest)?;)*
                Ok(())
            }             

            #[allow(non_snake_case)]
            fn encoding_length(&self) -> Result<usize> {
                let ($($type),*) = self;
                Ok(
                    0
                    $(+ $type.encoding_length()?)*
                )
            }
        }

        impl<$($type: Decode),*> Decode for ($($type),*) {
            #[allow(unused_mut, unused_variables)]
            fn decode<R: Read>(mut input: R) -> Result<Self> {
                Ok((
                    $($type::decode(&mut input)?),*
                ))
            }
        }
    }
}

tuple_impl!();
tuple_impl!(A, B);
tuple_impl!(A, B, C);
tuple_impl!(A, B, C, D);
tuple_impl!(A, B, C, D, E);
tuple_impl!(A, B, C, D, E, F);
tuple_impl!(A, B, C, D, E, F, G);

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

    #[test]
    fn encode_decode_option() {
        let value = Some(0x1234567890u64);
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[1, 0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90]);
        let decoded_value: Option<u64> = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);

        let value: Option<u64> = None;
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0]);
        let decoded_value: Option<u64> = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, None);
    }
}
