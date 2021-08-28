use failure::bail;

use crate::encoding::{Encode, Decode};
use crate::Result;

#[derive(Debug, Encode, Decode)]
pub enum Kind {
  Field,
  Method,
  This,
}

#[derive(Debug, Encode, Decode)]
pub enum Item<T, U> {
  Field(T),
  Method(U),
  This,
}

pub trait Query {
  type Query: Encode + Decode;

  fn query(&self, query: Self::Query) -> Result<()>;
}

pub trait FieldQuery {
  type Query: Encode + Decode;

  fn field_query(&self, query: Self::Query) -> Result<()>;
}
impl<T> FieldQuery for T {
  default type Query = ();

  default fn field_query(&self, _: Self::Query) -> Result<()> {
    bail!("No field queries implemented")
  }
}

pub trait MethodQuery {
  type Query: Encode + Decode;

  fn method_query(&self, query: Self::Query) -> Result<()>;
}
impl<T> MethodQuery for T {
  default type Query = ();

  default fn method_query(&self, _: Self::Query) -> Result<()> {
    bail!("No method queries implemented")
  }
}

impl<T: FieldQuery + MethodQuery> Query for T {
  type Query = Item<
    <Self as FieldQuery>::Query,
    <Self as MethodQuery>::Query,
  >;

  fn query(&self, query: Self::Query) -> Result<()> {
    match query {
      Item::Field(call) => self.field_query(call),
      Item::Method(call) => self.method_query(call),
      Item::This => Ok(()),
    }
  }
}
