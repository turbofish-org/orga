use super::State;
use crate::compat_mode;
use crate::Result;

/// A helper for flushing children in [State] implementations, used by the
/// derive macro.
pub struct Flusher<'a, W> {
    out: &'a mut W,
}

impl<'a, W> Flusher<'a, W>
where
    W: std::io::Write,
{
    /// Create a new [Flusher] for the given store.
    pub fn new(out: &'a mut W) -> Self {
        Self { out }
    }

    /// Flush a child.
    pub fn flush_child<U>(self, value: U) -> Result<Self>
    where
        U: State,
    {
        value.flush(self.out)?;

        Ok(self)
    }

    /// Flush a child as a different type, by converting to that type then
    /// flushing it.
    pub fn flush_child_as<T, U>(self, value: U) -> Result<Self>
    where
        T: State + From<U>,
        U: State,
    {
        let value: T = value.into();
        value.flush(self.out)?;

        Ok(self)
    }

    /// No-op for skipped children.
    pub fn flush_skipped_child<T>(self, _value: T) -> Result<Self> {
        Ok(self)
    }

    /// Flushes the child.
    pub fn flush_transparent_child<T: State>(self, value: T) -> Result<Self> {
        self.flush_child(value)
    }

    /// Writes the version byte.
    pub fn version(self, version: u8) -> Result<Self> {
        if !compat_mode() {
            self.out.write_all(&[version])?;
        }

        Ok(self)
    }
}
