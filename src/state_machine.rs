use crate::error::Result;
use crate::store::{Read, Write};

pub trait StateMachine<A, K, V>:
  Read<K, V> + Write<K, V>
{
  fn step(&mut self, action: A) -> Result<()>;
}
