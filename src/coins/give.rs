use crate::Result;

pub trait Give<V> {
    fn give(&mut self, value: V) -> Result<()>;
}
