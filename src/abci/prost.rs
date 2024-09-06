//! Prost-compatible encoding for ABCI types.

use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use prost::Message;
use std::io::{Error as IOError, ErrorKind as IOErrorKind, Read, Write};

/// An adapter for types which implement prost's [Message], allowing them to be
/// used with [Encode] and [Decode].
#[derive(Debug)]
pub struct Adapter<T: Message + Default>(pub(crate) T);

impl<T: Message + Default + 'static> State for Adapter<T> {
    fn attach(&mut self, _store: Store) -> crate::Result<()> {
        Ok(())
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> crate::Result<()> {
        self.encode_into(out)?;
        Ok(())
    }

    fn load(_store: Store, bytes: &mut &[u8]) -> crate::Result<Self> {
        Ok(Self::decode(bytes)?)
    }
}

impl<T: Message + Default> Encode for Adapter<T> {
    fn encode_into<W: Write>(&self, buf: &mut W) -> ed::Result<()> {
        let mut bytes = vec![];
        T::encode(&self.0, &mut bytes).unwrap(); // Prost encoding is infallible unless the buffer is full, and we're encoding
                                                 // into a vec, so this is safe.
        buf.write_all(&bytes)?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(T::encoded_len(&self.0))
    }
}

impl<T: Message + Default> Adapter<T> {
    /// Consumes the adapter and returns the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Message + Default> Decode for Adapter<T> {
    fn decode<R: Read>(mut input: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        input.read_to_end(&mut bytes)?;
        let decoded =
            T::decode(bytes.as_slice()).map_err(|e| IOError::new(IOErrorKind::InvalidData, e))?;

        Ok(Adapter(decoded))
    }
}

impl<T: Message + Default> std::ops::Deref for Adapter<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Message + Default> std::ops::DerefMut for Adapter<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
