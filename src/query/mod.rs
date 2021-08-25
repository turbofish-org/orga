use failure::bail;

use crate::encoding::{Encode, Decode};
use crate::Result;

pub struct Field;
pub struct Method;
pub struct This;

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

pub trait Query<T = Kind> {
  type Query: Encode + Decode;
  type Res: Encode + Decode;

  fn query(&self, query: Self::Query) -> Result<Self::Res>;
}

impl<T> Query<This> for T {
  default type Query = ();
  default type Res = ();

  default fn query(&self, _: Self::Query) -> Result<Self::Res> {
    bail!("This query not implemented")
  }
}

impl<T: Clone + Encode + Decode> Query<This> for T {
  type Query = ();
  type Res = T;

  fn query(&self, _: ()) -> Result<T> {
    Ok(self.clone())
  }
}

impl<T> Query<Field> for T {
  default type Query = ();
  default type Res = ();

  default fn query(&self, _: Self::Query) -> Result<Self::Res> {
    bail!("No field queries implemented")
  }
}

impl<T> Query<Method> for T {
  default type Query = ();
  default type Res = ();

  default fn query(&self, _: Self::Query) -> Result<Self::Res> {
    bail!("No method queries implemented")
  }
}

impl<T: Query<Field> + Query<Method> + Query<This>> Query for T {
  type Query = Item<
    <Self as Query<Field>>::Query,
    <Self as Query<Method>>::Query,
    <Self as Query<This>>::Query,
  >;
  type Res = Item<
    <Self as Query<Field>>::Res,
    <Self as Query<Method>>::Res,
    <Self as Query<This>>::Res,
  >;

  fn query(&self, query: Self::Query) -> Result<Self::Res> {
    Ok(match query {
      Item::Field(call) => Item::Field(Query::<Field>::query(self, call)?),
      Item::Method(call) => Item::Method(Query::<Method>::query(self, call)?),
      Item::This(call) => Item::This(Query::<This>::query(self, call)?),
    })
  }
}
