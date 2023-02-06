use super::State;
use crate::compat_mode;
use crate::Result;

pub struct Flusher<'a, W> {
    out: &'a mut W,
}

impl<'a, W> Flusher<'a, W>
where
    W: std::io::Write,
{
    pub fn new(out: &'a mut W) -> Self {
        Self { out }
    }

    pub fn flush_child<U>(self, value: U) -> Result<Self>
    where
        U: State,
    {
        value.flush(self.out)?;

        Ok(self)
    }

    pub fn flush_child_as<T, U>(self, value: U) -> Result<Self>
    where
        T: State + From<U>,
        U: State,
    {
        let value: T = value.into();
        value.flush(self.out)?;

        Ok(self)
    }

    pub fn flush_skipped_child<T>(self, _value: T) -> Result<Self> {
        Ok(self)
    }

    pub fn flush_transparent_child<T: State>(self, value: T) -> Result<Self> {
        self.flush_child(value)
    }

    pub fn version(self, version: u8) -> Result<Self> {
        if !compat_mode() {
            self.out.write_all(&[version])?;
        }

        Ok(self)
    }
}
