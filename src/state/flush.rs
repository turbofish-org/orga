use super::state2::State;
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

    pub fn flush_child<U>(self, value: &mut U) -> Result<Self>
    where
        U: State,
    {
        value.flush(self.out)?;

        Ok(self)
    }

    pub fn version(self, version: u8) -> Result<Self> {
        self.out.write_all(&[version])?;

        Ok(self)
    }
}
