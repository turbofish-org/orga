use crate::Result;
use ed::{Encode, Decode};

pub trait Call {
  type Call: Encode + Decode;
  // TODO: type Res: Encode + Decode;

  fn call(&mut self, call: Self::Call) -> Result<()>;
}
