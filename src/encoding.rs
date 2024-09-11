use crate::describe::Describe;
use crate::migrate::Migrate;
use crate::query::FieldQuery;
use crate::state::State;
use crate::store::Store;
pub use ed::*;
pub use orga_macros::VersionedEncoding;
pub mod decoder;
pub mod encoder;

use derive_more::{Deref, DerefMut, Into};
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};

#[derive(
    Deref,
    DerefMut,
    Encode,
    Into,
    Default,
    Clone,
    Debug,
    FieldQuery,
    PartialEq,
    Hash,
    Eq,
    Describe,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
pub struct LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone + 'static,
    T: Encode + Decode + Terminated + 'static,
{
    #[serde(skip)]
    len: P,

    #[deref]
    #[deref_mut]
    #[into]
    values: Vec<T>,
}

impl<P, T> Migrate for LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone + 'static,
    T: Encode + Decode + Terminated + 'static,
{
}

impl<P, T> LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: Encode + Decode + Terminated,
{
    pub fn new(len: P, values: Vec<T>) -> Self {
        LengthVec { len, values }
    }
}

impl<P, T> Decode for LengthVec<P, T>
where
    P: Encode + Decode + Terminated + TryInto<usize> + Clone,
    T: Encode + Decode + Terminated,
{
    fn decode<R: std::io::Read>(mut input: R) -> Result<Self> {
        let len = P::decode(&mut input)?;

        let len_usize = len
            .clone()
            .try_into()
            .map_err(|_| Error::UnexpectedByte(80))?;

        let mut values = Vec::with_capacity(len_usize);
        for _ in 0..len_usize {
            let value = T::decode(&mut input)?;
            values.push(value);
        }

        Ok(LengthVec { len, values })
    }
}

impl<P, T> Terminated for LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
    T: Encode + Decode + Terminated,
{
}

impl<P, T> State for LengthVec<P, T>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone + 'static,
    T: Encode + Decode + Terminated + 'static,
{
    fn attach(&mut self, _store: Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.len.encode_into(out)?;
        self.values.encode_into(out)?;

        Ok(())
    }

    fn load(_store: Store, mut bytes: &mut &[u8]) -> crate::Result<Self> {
        let len = P::decode(&mut bytes)?;
        let len_usize = len
            .clone()
            .try_into()
            .map_err(|_| Error::UnexpectedByte(80))?;

        let mut values = Vec::with_capacity(len_usize);
        for _ in 0..len_usize {
            let value = T::decode(&mut bytes)?;
            values.push(value);
        }

        Ok(LengthVec { len, values })
    }
}

impl<P> TryFrom<String> for LengthVec<P, u8>
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
{
    type Error = crate::Error;

    fn try_from(value: String) -> crate::Result<Self> {
        value.into_bytes().try_into()
    }
}

impl<P> TryFrom<LengthVec<P, u8>> for String
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
{
    type Error = crate::Error;

    fn try_from(value: LengthVec<P, u8>) -> crate::Result<Self> {
        String::from_utf8(value.values).map_err(|_| crate::Error::Overflow)
    }
}

impl<P> TryFrom<&str> for LengthVec<P, u8>
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
{
    type Error = crate::Error;

    fn try_from(value: &str) -> crate::Result<Self> {
        value.to_string().try_into()
    }
}

// impl<P, T> Describe for LengthVec<P, T>
// where
//     P: State + Encode + Decode + TryInto<usize> + Terminated + Clone +
// 'static,     T: State + Encode + Decode + Terminated + 'static,
// {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>().build()
//     }
// }

impl<P, T> TryFrom<Vec<T>> for LengthVec<P, T>
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
    T: State + Encode + Decode + Terminated,
{
    type Error = crate::Error;

    fn try_from(values: Vec<T>) -> crate::Result<Self> {
        let len = values
            .len()
            .try_into()
            .map_err(|_| crate::Error::Overflow)?;
        Ok(Self { len, values })
    }
}

#[derive(
    Deref,
    DerefMut,
    Into,
    Default,
    Clone,
    Debug,
    FieldQuery,
    PartialEq,
    Hash,
    Eq,
    Describe,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
pub struct LengthString<P>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone + 'static,
{
    #[serde(skip)]
    len: P,

    #[deref]
    #[deref_mut]
    #[into]
    inner: String,
}

impl<P> Migrate for LengthString<P> where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone + 'static
{
}

impl<P> LengthString<P>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
{
    pub fn new(len: P, inner: String) -> Self {
        LengthString { len, inner }
    }
}

impl<P> Decode for LengthString<P>
where
    P: Encode + Decode + Terminated + TryInto<usize> + Clone,
{
    fn decode<R: std::io::Read>(mut input: R) -> Result<Self> {
        let len = P::decode(&mut input)?;

        let len_usize = len
            .clone()
            .try_into()
            .map_err(|_| Error::UnexpectedByte(80))?;

        let mut inner = String::with_capacity(len_usize);
        for _ in 0..len_usize {
            let value = u8::decode(&mut input)?;
            inner.push(value as char);
        }

        Ok(LengthString { len, inner })
    }
}

impl<P> Encode for LengthString<P>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone,
{
    fn encode_into<W: std::io::Write>(&self, mut out: &mut W) -> Result<()> {
        self.len.encode_into(&mut out)?;
        for c in self.inner.chars() {
            (c as u8).encode_into(&mut out)?;
        }

        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        let mut len = self.len.encoding_length()?;
        for c in self.inner.chars() {
            len += (c as u8).encoding_length()?;
        }

        Ok(len)
    }
}

impl<P> Terminated for LengthString<P> where P: Encode + Decode + TryInto<usize> + Terminated + Clone
{}

impl<P> State for LengthString<P>
where
    P: Encode + Decode + TryInto<usize> + Terminated + Clone + 'static,
{
    fn attach(&mut self, _store: Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.len.encode_into(out)?;
        // TODO: non-utf8 support?
        self.inner.as_bytes().encode_into(out)?;

        Ok(())
    }

    fn load(_store: Store, mut bytes: &mut &[u8]) -> crate::Result<Self> {
        let len = P::decode(&mut bytes)?;
        let len_usize = len
            .clone()
            .try_into()
            .map_err(|_| Error::UnexpectedByte(80))?;

        let mut inner = String::with_capacity(len_usize);
        for _ in 0..len_usize {
            let value = u8::decode(&mut bytes)?;
            inner.push(value as char);
        }

        Ok(LengthString { len, inner })
    }
}

impl<P> TryFrom<&str> for LengthString<P>
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
{
    type Error = crate::Error;

    fn try_from(value: &str) -> crate::Result<Self> {
        value.to_string().try_into()
    }
}

// impl<P> Describe for LengthString<P>
// where
//     P: State + Encode + Decode + TryInto<usize> + Terminated + Clone +
// 'static, {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>().build()
//     }
// }

impl<P> TryFrom<String> for LengthString<P>
where
    P: State + Encode + Decode + TryInto<usize> + TryFrom<usize> + Terminated + Clone,
{
    type Error = crate::Error;

    fn try_from(inner: String) -> crate::Result<Self> {
        let len = inner.len().try_into().map_err(|_| crate::Error::Overflow)?;
        Ok(Self { len, inner })
    }
}

#[derive(Clone, Debug)]
pub struct Adapter<T>(pub T);

impl<T> State for Adapter<T>
where
    Self: Encode + Decode + 'static,
{
    fn attach(&mut self, _store: crate::store::Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.encode_into(out)?;
        Ok(())
    }

    fn load(_store: crate::store::Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        Ok(Self::decode(bytes)?)
    }
}

#[derive(Clone, Debug, Deref, Serialize, Default)]
#[serde(transparent)]
pub struct ByteTerminatedString<const B: u8, T: FromStr + ToString = String>(pub T);

impl<const B: u8, T: FromStr + ToString + 'static> Migrate for ByteTerminatedString<B, T> {}

impl<const B: u8, T: FromStr + ToString + 'static> Describe for ByteTerminatedString<B, T> {
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl<T: FromStr + ToString, const B: u8> Encode for ByteTerminatedString<B, T> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        for byte in self.0.to_string().as_bytes() {
            debug_assert!(byte != &B);
        }

        dest.write_all(self.0.to_string().as_bytes())?;
        dest.write_all(&[B])?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.0.to_string().len() + 1)
    }
}

impl<T: FromStr + ToString, const B: u8> Decode for ByteTerminatedString<B, T> {
    fn decode<R: std::io::Read>(input: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        for byte in input.bytes() {
            let byte = byte?;
            if byte == B {
                break;
            }
            bytes.push(byte);
        }

        let inner = String::from_utf8(bytes)
            .map_err(|_| ed::Error::UnexpectedByte(4))?
            .parse()
            .map_err(|_| ed::Error::UnexpectedByte(4))?;

        Ok(Self(inner))
    }
}

impl<T: FromStr + ToString + 'static, const B: u8> State for ByteTerminatedString<B, T>
where
    Self: Encode + Decode,
{
    fn attach(&mut self, _store: crate::store::Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.encode_into(out)?;
        Ok(())
    }

    fn load(_store: crate::store::Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        Ok(Self::decode(bytes)?)
    }
}

impl<T: FromStr + ToString, const B: u8> Terminated for ByteTerminatedString<B, T> {}

impl<T: FromStr + ToString, const B: u8> From<T> for ByteTerminatedString<B, T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Deref, Serialize, Default)]
#[serde(transparent)]
pub struct EofTerminatedString<T: FromStr + ToString = String>(pub T);

impl<T: FromStr + ToString + 'static> Describe for EofTerminatedString<T> {
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl<T: FromStr + ToString + 'static> Migrate for EofTerminatedString<T> {}

impl<T: FromStr + ToString> Encode for EofTerminatedString<T> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        dest.write_all(self.0.to_string().as_bytes())?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(self.0.to_string().len() + 1)
    }
}

impl<T: FromStr + ToString> Decode for EofTerminatedString<T> {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut string = String::new();
        input.read_to_string(&mut string)?;

        let inner = string.parse().map_err(|_| ed::Error::UnexpectedByte(4))?;

        Ok(Self(inner))
    }
}

impl<T: FromStr + ToString> Terminated for EofTerminatedString<T> {}

impl<T: FromStr + ToString + 'static> State for EofTerminatedString<T>
where
    Self: Encode + Decode,
{
    fn attach(&mut self, _store: crate::store::Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.encode_into(out)?;
        Ok(())
    }

    fn load(_store: crate::store::Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        Ok(Self::decode(bytes)?)
    }
}

impl<T: FromStr + ToString> From<T> for EofTerminatedString<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T: FromStr + ToString> EofTerminatedString<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct FixedString<const S: &'static str>;

impl<const S: &'static str> Describe for FixedString<S> {
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>().build()
    }
}

impl<const S: &'static str> Migrate for FixedString<S> {}

impl<const S: &'static str> Encode for FixedString<S> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        dest.write_all(S.as_bytes())?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(S.len())
    }
}

impl<const S: &'static str> Decode for FixedString<S> {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut bytes = vec![0; S.len()];
        input.read_exact(&mut bytes)?;

        if bytes != S.as_bytes() {
            return Err(ed::Error::UnexpectedByte(3));
        }

        Ok(Self)
    }
}

impl<const S: &'static str> Terminated for FixedString<S> {}

impl<const S: &'static str> State for FixedString<S>
where
    Self: Encode + Decode,
{
    fn attach(&mut self, _store: crate::store::Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.encode_into(out)?;
        Ok(())
    }

    fn load(_store: crate::store::Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        Ok(Self::decode(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    type CommaTerminatedU64 = ByteTerminatedString<b',', u64>;

    #[test]
    fn byte_terminated_string_encode_decode() {
        let value: CommaTerminatedU64 = ByteTerminatedString(1234);

        let mut bytes = value.encode().unwrap();
        assert_eq!(bytes, b"1234,");

        bytes.extend_from_slice(b"567,8,");
        let decoded = CommaTerminatedU64::decode(&bytes[..]).unwrap();
        assert_eq!(*decoded, *value);
    }

    #[test]
    fn byte_terminated_string_state() {
        let value: CommaTerminatedU64 = ByteTerminatedString(1234);

        let mut bytes = vec![];
        value.clone().flush(&mut bytes).unwrap();
        assert_eq!(bytes, b"1234,");

        bytes.extend_from_slice(b"567,8,");
        let decoded = CommaTerminatedU64::load(Store::default(), &mut &bytes[..]).unwrap();
        assert_eq!(*decoded, *value);
    }

    #[test]
    fn eof_terminated_string_encode_decode() {
        let value: EofTerminatedString<u64> = EofTerminatedString(1234);

        let bytes = value.encode().unwrap();
        assert_eq!(bytes, b"1234");

        let decoded = EofTerminatedString::<u64>::decode(&bytes[..]).unwrap();
        assert_eq!(*decoded, *value);
    }

    #[test]
    fn eof_terminated_string_state() {
        let value: EofTerminatedString<u64> = EofTerminatedString(1234);

        let mut bytes = vec![];
        value.clone().flush(&mut bytes).unwrap();
        assert_eq!(bytes, b"1234");

        let decoded = EofTerminatedString::<u64>::load(Store::default(), &mut &bytes[..]).unwrap();
        assert_eq!(*decoded, *value);
    }

    #[test]
    fn string_roundtrip() -> crate::Result<()> {
        let value = "hello";
        let lv: LengthVec<u8, u8> = value.try_into()?;
        assert_eq!(lv.len, 5);
        assert_eq!(lv.values, vec![b'h', b'e', b'l', b'l', b'o']);
        let value: String = lv.try_into()?;
        assert_eq!(value, "hello");

        Ok(())
    }
}
