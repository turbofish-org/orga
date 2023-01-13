use super::Encode;
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
        let mut value: T = value.into();
        self.encode_child(&value)
    }

    pub fn version(self, version: u8) -> Result<Self> {
        self.out.write_all(&[version])?;

        Ok(self)
    }
}
