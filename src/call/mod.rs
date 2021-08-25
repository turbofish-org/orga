use crate::Result;
use ed::{Encode, Decode};
use failure::bail;

pub struct Field;
pub struct Method;

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

pub trait Call<T = Kind> {
  type Call: Encode + Decode;
  // TODO: type Res: Encode + Decode;

  fn call(&mut self, call: Self::Call) -> Result<()>;
}

default impl<T> Call<Field> for T {
  type Call = ();

  fn call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No field calls implemented")
  }
}

default impl<T> Call<Method> for T {
  type Call = ();

  fn call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No method calls implemented")
  }
}

impl<T: Call<Field> + Call<Method>> Call for T {
  type Call = Item<
    <Self as Call<Field>>::Call,
    <Self as Call<Method>>::Call,
  >;

  fn call(&mut self, call: Self::Call) -> Result<()> {
    match call {
      Item::Field(call) => Call::<Field>::call(self, call),
      Item::Method(call) => Call::<Method>::call(self, call),
    }
  }
}

impl Call<Field> for u32 {
  type Call = ();

  fn call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No field calls implemented")
  }
}
impl Call<Method> for u32 {
  type Call = ();

  fn call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No method calls implemented")
  }
}
