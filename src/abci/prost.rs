use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use prost::Message;
use std::io::{Error as IOError, ErrorKind as IOErrorKind, Read, Write};

#[derive(Debug)]
pub struct Adapter<T: Message + Default>(T);

impl<T: Message + Default> State for Adapter<T> {
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> crate::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> crate::Result<Self::Encoding> {
        Ok(self)
    }
}

impl<T: Message + Default> Encode for Adapter<T> {
    fn encode_into<W: Write>(&self, buf: &mut W) -> ed::Result<()> {
        let mut bytes = vec![];
        T::encode(&self.0, &mut bytes).unwrap(); // Prost encoding is infallible unless the buffer is full, and we're encoding into a vec, so this is safe.
        buf.write_all(&bytes)?;
        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(T::encoded_len(&self.0))
    }
}

impl<T: Message + Default> Adapter<T> {
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

impl<T: Message + Default> From<T> for Adapter<T> {
    fn from(t: T) -> Self {
        Adapter(t)
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
