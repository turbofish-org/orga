use crate::Result;
use failure::{bail, format_err};
use std::io::{Read, Write};

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
    };
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
            None => dest.write_all(&[0]).map_err(|err| format_err!("{}", err)),
            Some(value) => {
                dest.write_all(&[1]).map_err(|err| format_err!("{}", err))?;
                value.encode_into(dest)
            }
        }
    }

    fn encoding_length(&self) -> Result<usize> {
        match self {
            None => Ok(1),
            Some(value) => Ok(1 + value.encoding_length()?),
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
            byte => bail!("Unexpected byte {}", byte),
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

macro_rules! array_impl {
    ($length:expr) => {
        impl<T: Encode> Encode for [T; $length] {
            #[allow(non_snake_case, unused_mut, unused_variables)]
            fn encode_into<W: Write>(&self, mut dest: &mut W) -> Result<()> {
                for element in self[..].iter() {
                    element.encode_into(&mut dest)?;
                }
                Ok(())
            }

            #[allow(non_snake_case)]
            fn encoding_length(&self) -> Result<usize> {
                let mut sum = 0;
                for element in self[..].iter() {
                    sum += element.encoding_length()?;
                }
                Ok(sum)
            }
        }

        // TODO: support T without Default + Copy
        impl<T: Decode + Default + Copy> Decode for [T; $length] {
            #[allow(unused_mut, unused_variables)]
            fn decode<R: Read>(mut input: R) -> Result<Self> {
                let mut array = [Default::default(); $length];
                for i in 0..$length {
                    array[i] = T::decode(&mut input)?;
                }
                Ok(array)
            }
        }
    };
}

array_impl!(1);
array_impl!(2);
array_impl!(3);
array_impl!(4);
array_impl!(5);
array_impl!(6);
array_impl!(7);
array_impl!(8);
array_impl!(9);
array_impl!(10);
array_impl!(11);
array_impl!(12);
array_impl!(13);
array_impl!(14);
array_impl!(15);
array_impl!(16);
array_impl!(17);
array_impl!(18);
array_impl!(19);
array_impl!(20);
array_impl!(21);
array_impl!(22);
array_impl!(23);
array_impl!(24);
array_impl!(25);
array_impl!(26);
array_impl!(27);
array_impl!(28);
array_impl!(29);
array_impl!(30);
array_impl!(31);
array_impl!(32);
array_impl!(33);
array_impl!(64);
array_impl!(128);
array_impl!(256);

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
        assert_eq!(
            bytes.as_slice(),
            &[1, 0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90]
        );
        let decoded_value: Option<u64> = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);

        let value: Option<u64> = None;
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0]);
        let decoded_value: Option<u64> = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, None);
    }

    #[test]
    fn encode_decode_tuple() {
        let value: (u16, u16) = (1, 2);
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0, 1, 0, 2]);
        let decoded_value: (u16, u16) = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);

        let value = ();
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice().len(), 0);
        let decoded_value: () = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);
    }

    #[test]
    fn encode_decode_array() {
        let value: [u16; 4] = [1, 2, 3, 4];
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0, 1, 0, 2, 0, 3, 0, 4]);
        let decoded_value: [u16; 4] = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);
    }
}
