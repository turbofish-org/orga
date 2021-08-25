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

impl<T> Call<Method> for T {
  default type Call = ();

  default fn call(&mut self, _: Self::Call) -> Result<()> {
    bail!("No method calls implemented")
  }
}

struct Foo;
impl Call<Method> for Foo {
  type Call = u64;
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

// pub trait Query<T = Kind> {
//   type Query: Encode + Decode;
//   type Res: Encode + Decode;

//   fn query(&self, query: Self::Query) -> Result<Self::Res>;
// }
