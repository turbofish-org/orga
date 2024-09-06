use super::Encode;
use crate::compat_mode;
use ed::Result;

/// A helper for encoding versioned [Encode] types.
pub struct Encoder<'a, W> {
    out: &'a mut W,
}

impl<'a, W> Encoder<'a, W>
where
    W: std::io::Write,
{
    /// Create a new encoder.
    pub fn new(out: &'a mut W) -> Self {
        Self { out }
    }

    /// Encode the child.
    pub fn encode_child<U>(self, value: &U) -> Result<Self>
    where
        U: Encode,
    {
        value.encode_into(self.out)?;

        Ok(self)
    }

    /// Convert the child to another type, then encode it.
    pub fn encode_child_as<T, U>(self, value: U) -> Result<Self>
    where
        T: Encode + From<U>,
    {
        let value: T = value.into();
        self.encode_child(&value)
    }

    /// Write the version byte.
    pub fn version(self, version: u8) -> Result<Self> {
        if !compat_mode() {
            self.out.write_all(&[version])?;
        }

        Ok(self)
    }

    /// Returns the encoding length of the value when encoded as the given
    /// type.
    pub fn encoding_length_as<T, U>(value: U) -> Result<usize>
    where
        T: Encode + From<U>,
    {
        let value: T = value.into();
        value.encoding_length()
    }
}
