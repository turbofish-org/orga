use super::Encode;
use crate::compat_mode;
use ed::Result;

pub struct Encoder<'a, W> {
    out: &'a mut W,
}

impl<'a, W> Encoder<'a, W>
where
    W: std::io::Write,
{
    pub fn new(out: &'a mut W) -> Self {
        Self { out }
    }

    pub fn encode_child<U>(self, value: &U) -> Result<Self>
    where
        U: Encode,
    {
        value.encode_into(self.out)?;

        Ok(self)
    }

    pub fn encode_child_as<T, U>(self, value: U) -> Result<Self>
    where
        T: Encode + From<U>,
    {
        let value: T = value.into();
        self.encode_child(&value)
    }

    pub fn version(self, version: u8) -> Result<Self> {
        if !compat_mode() {
            self.out.write_all(&[version])?;
        }

        Ok(self)
    }

    pub fn encoding_length_as<T, U>(value: U) -> Result<usize>
    where
        T: Encode + From<U>,
    {
        let value: T = value.into();
        value.encoding_length()
    }
}
