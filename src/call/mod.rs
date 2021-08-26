use crate::Result;
use ed::{Encode, Decode};
use failure::bail;

#[derive(Debug, Encode, Decode)]
pub enum Kind {
  Field,
  Method,
}

#[derive(Debug, Encode, Decode)]
pub enum Item<T, U> {
  Field(T),
  Method(U),
}

pub trait Call {
  type Call: Encode + Decode;
  // TODO: type Res: Encode + Decode;

  fn call(&mut self, call: Self::Call) -> Result<()>;
}

pub trait FieldCall {
  type Call: Encode + Decode;

  fn field_call(&mut self, call: Self::Call) -> Result<()>;
}
impl<T> FieldCall for T {
  default type Call = ();

  default fn field_call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No field calls implemented")
  }
}

pub trait MethodCall {
  type Call: Encode + Decode;

  fn method_call(&mut self, call: Self::Call) -> Result<()>;
}
impl<T> MethodCall for T {
  default type Call = ();

  default fn method_call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No method calls implemented")
  }
}

impl<T: FieldCall + MethodCall> Call for T {
  type Call = Item<
    <Self as FieldCall>::Call,
    <Self as MethodCall>::Call,
  >;

  fn call(&mut self, call: Self::Call) -> Result<()> {
    match call {
      Item::Field(call) => self.field_call(call),
      Item::Method(call) => self.method_call(call),
    }
  }
}
