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
pub enum Item<T, U, V> {
  Field(T),
  Method(U),
  This(V),
}

pub trait Query {
  type Query: Encode + Decode;
  type Res;

  fn query(&self, query: Self::Query) -> Result<Self::Res>;
}

pub trait ThisQuery: Sized {
  fn this_query(&self) -> Result<Self>;
}
impl<T> ThisQuery for T {
  default fn this_query(&self) -> Result<Self> {
    bail!("This query not implemented")
  }
}
impl<T: Clone + Encode + Decode> ThisQuery for T {
  fn this_query(&self) -> Result<T> {
    Ok(self.clone())
  }
}

pub trait FieldQuery {
  type Query: Encode + Decode;
  type Res;

  fn field_query(&self, query: Self::Query) -> Result<Self::Res>;
}
impl<T> FieldQuery for T {
  default type Query = ();
  default type Res = ();

  default fn field_query(&self, _: Self::Query) -> Result<Self::Res> {
    bail!("No field queries implemented")
  }
}

pub trait MethodQuery {
  type Query: Encode + Decode;
  type Res;

  fn method_query(&self, query: Self::Query) -> Result<Self::Res>;
}
impl<T> MethodQuery for T {
  default type Query = ();
  default type Res = ();

  default fn method_query(&self, _: Self::Query) -> Result<Self::Res> {
    bail!("No method queries implemented")
  }
}

impl<T> Query for T
where
  T: ThisQuery + FieldQuery + MethodQuery,
  T: Sized,
{
  type Query = Item<
    <Self as FieldQuery>::Query,
    <Self as MethodQuery>::Query,
    (),
  >;
  type Res = Item<
    <Self as FieldQuery>::Res,
    <Self as MethodQuery>::Res,
    Self,
  >;

  fn query(&self, query: Self::Query) -> Result<Self::Res> {
    Ok(match query {
      Item::Field(call) => Item::Field(self.field_query(call)?),
      Item::Method(call) => Item::Method(self.method_query(call)?),
      Item::This(_) => Item::This(self.this_query()?),
    })
  }
}
