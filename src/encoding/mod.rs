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

    fn decode_into<R: Read>(&mut self, input: R) -> Result<()> {
        let value = Self::decode(input)?;
        *self = value;
        Ok(())
    }
}

pub trait Terminated {}

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

        impl Terminated for $type {}
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


impl Encode for bool {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
        let bytes = [ *self as u8 ];
        dest.write_all(&bytes[..])?;
        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        Ok(1)
    }
}

impl Decode for bool {
    fn decode<R: Read>(mut input: R) -> Result<Self> {
        let mut buf = [0; 1];
        input.read_exact(&mut buf[..])?;
        match buf[0] {
            0 => Ok(false),
            1 => Ok(true),
            byte => bail!("Unexpected byte {}", byte)
        }
    }
}

impl Terminated for bool {}

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
    fn decode<R: Read>(input: R) -> Result<Self> {
        let mut option: Option<T> = None;
        option.decode_into(input)?;
        Ok(option)
    }

    fn decode_into<R: Read>(&mut self, mut input: R) -> Result<()> {
        let mut byte = [0; 1];
        input.read_exact(&mut byte[..])?;

        match byte[0] {
            0 => *self = None,
            1 => match self {
                None => *self = Some(T::decode(input)?),
                Some(value) => value.decode_into(input)?
            },
            byte => bail!("Unexpected byte {}", byte)
        };

        Ok(())
    }
}

impl<T: Terminated> Terminated for Option<T> {}

impl Encode for () {
    fn encode_into<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        Ok(0)
    }
}

impl Decode for () {
    fn decode<R: Read>(_: R) -> Result<Self> {
        Ok(())
    }
}

impl Terminated for () {}

macro_rules! tuple_impl {
    ($( $type:ident ),*; $last_type:ident) => {
        impl<$($type: Encode + Terminated,)* $last_type: Encode> Encode for ($($type,)* $last_type,) {
            #[allow(non_snake_case, unused_mut)]
            fn encode_into<W: Write>(&self, mut dest: &mut W) -> Result<()> {
                let ($($type,)* $last_type,) = self;
                $($type.encode_into(&mut dest)?;)*
                $last_type.encode_into(dest)
            }

            #[allow(non_snake_case)]
            fn encoding_length(&self) -> Result<usize> {
                let ($($type,)* $last_type,) = self;
                Ok(
                    $($type.encoding_length()? +)*
                    $last_type.encoding_length()?
                )
            }
        }

        impl<$($type: Decode + Terminated,)* $last_type: Decode> Decode for ($($type,)* $last_type,) {
            #[allow(unused_mut)]
            fn decode<R: Read>(mut input: R) -> Result<Self> {
                Ok((
                    $($type::decode(&mut input)?,)*
                    $last_type::decode(input)?,
                ))
            }

            #[allow(non_snake_case, unused_mut)]
            fn decode_into<R: Read>(&mut self, mut input: R) -> Result<()> {
                let ($($type,)* $last_type,) = self;
                $($type.decode_into(&mut input)?;)*
                $last_type.decode_into(input)?;
                Ok(())
            }
        }

        impl<$($type: Terminated,)* $last_type: Terminated> Terminated for ($($type,)* $last_type,) {}
    }
}

tuple_impl!(; A);
tuple_impl!(A; B);
tuple_impl!(A, B; C);
tuple_impl!(A, B, C; D);
tuple_impl!(A, B, C, D; E);
tuple_impl!(A, B, C, D, E; F);
tuple_impl!(A, B, C, D, E, F; G);

macro_rules! array_impl {
    ($length:expr) => {
        impl<T: Encode + Terminated> Encode for [T; $length] {
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

        impl<T: Decode + Terminated + Default + Copy> Decode for [T; $length] {
            fn decode<R: Read>(input: R) -> Result<Self> {
                let mut array = [Default::default(); $length];
                array.decode_into(input)?;
                Ok(array)
            }

            fn decode_into<R: Read>(&mut self, mut input: R) -> Result<()> {
                for i in 0..$length {
                    T::decode_into(&mut self[i], &mut input)?;
                }
                Ok(())
            }
        }

        impl<T: Terminated> Terminated for [T; $length] {}
    };
}

array_impl!(0);
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

impl<T: Encode + Terminated> Encode for Vec<T> {
    fn encode_into<W: Write>(&self, mut dest: &mut W) -> Result<()> {
        for element in self.iter() {
            element.encode_into(&mut dest)?;
        }
        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        let mut sum = 0;
        for element in self.iter() {
            sum += element.encoding_length()?;
        }
        Ok(sum)
    }
}

impl<T: Decode + Terminated> Decode for Vec<T> {
    fn decode<R: Read>(input: R) -> Result<Self> {
        let mut vec = vec![];
        vec.decode_into(input)?;
        Ok(vec)
    }

    fn decode_into<R: Read>(&mut self, input: R) -> Result<()> {
        let mut input = EofReader::new(input);
        let old_len = self.len();

        let mut i = 0;
        while !input.eof()? {
            if i < old_len {
                self[i].decode_into(&mut input)?;
            } else {
                let el = T::decode(&mut input)?;
                self.push(el);
            }

            i += 1;
        }

        if i < old_len {
            self.truncate(i);
        }

        Ok(())
    }
}

impl<T: Encode + Terminated> Encode for [T] {
    fn encode_into<W: Write>(&self, mut dest: &mut W) -> Result<()> {
        for element in self[..].iter() {
            element.encode_into(&mut dest)?;
        }
        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        let mut sum = 0;
        for element in self[..].iter() {
            sum += element.encoding_length()?;
        }
        Ok(sum)
    }
}

struct EofReader<R: Read> {
    inner: R,
    next_byte: Option<u8>
}

impl<R: Read> EofReader<R> {
    fn new(read: R) -> Self {
        EofReader {
            inner: read,
            next_byte: None
        }
    }

    fn eof(&mut self) -> Result<bool> {
        if let Some(_) = self.next_byte {
            return Ok(false);
        }

        let mut buf = [0];
        let bytes_read = self.inner.read(&mut buf)?;
        if bytes_read == 0 {
            Ok(true)
        } else {
            self.next_byte = Some(buf[0]);
            Ok(false)
        }
    }
}

type IoResult<T> = std::result::Result<T, std::io::Error>;
impl<R: Read> Read for EofReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if buf.len() == 0 {
            return Ok(0)
        }

        match self.next_byte.take() {
            Some(byte) => {
                buf[0] = byte;
                let bytes_read = self.inner.read(&mut buf[1..])?;
                Ok(1 + bytes_read)
            },
            None => self.inner.read(buf)
        }
    }
}

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

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn encode_decode_array_eof_length() {
        let bytes = [0, 1, 0, 2, 0, 3];
        let _: [u16; 4] = Decode::decode(&bytes[..]).unwrap();
    }

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn encode_decode_array_eof_element() {
        let bytes = [0, 1, 0, 2, 0, 3, 0];
        let _: [u16; 4] = Decode::decode(&bytes[..]).unwrap();
    }

    #[test]
    fn encode_decode_vec() {
        let value: Vec<u16> = vec![1, 2, 3, 4];
        let bytes = value.encode().unwrap();
        assert_eq!(bytes.as_slice(), &[0, 1, 0, 2, 0, 3, 0, 4]);
        let decoded_value: Vec<u16> = Decode::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded_value, value);
    }

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn encode_decode_vec_eof_element() {
        let bytes = [0, 1, 0, 2, 0, 3, 0];
        let _: Vec<u16> = Decode::decode(&bytes[..]).unwrap();
    }
}
