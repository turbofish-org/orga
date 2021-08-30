use failure::bail;

use crate::encoding::{Encode, Decode};
use crate::Result;

#[derive(Debug, Encode, Decode)]
pub enum Item<T, U, V> {
  Field(T),
  Method(U),
  Chained(V),
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
  type ChainedQuery: Encode + Decode;

  fn method_query(&self, query: Self::Query) -> Result<()>;

  fn chained_query(&self, query: Self::ChainedQuery) -> Result<()>;
}
impl<T> MethodQuery for T {
  default type Query = ();
  default type ChainedQuery = ();

  default fn method_query(&self, _: Self::Query) -> Result<()> {
    bail!("No method queries implemented")
  }

  default fn chained_query(&self, _: Self::ChainedQuery) -> Result<()> {
    bail!("No chained method queries implemented")
  }
}

impl<T: FieldQuery + MethodQuery> Query for T {
  type Query = Item<
    <Self as FieldQuery>::Query,
    <Self as MethodQuery>::Query,
    <Self as MethodQuery>::ChainedQuery,
  >;

  fn query(&self, query: Self::Query) -> Result<()> {
    match query {
      Item::Field(call) => self.field_query(call),
      Item::Method(call) => self.method_query(call),
      Item::Chained(call) => self.chained_query(call),
      Item::This => Ok(()),
    }
  }
}

impl<T: FieldQuery> FieldQuery for Result<T> {
  type Query = T::Query;

  fn field_query(&self, query: Self::Query) -> Result<()> {
    match self {
      Ok(inner) => inner.field_query(query),
      Err(err) => Err(failure::format_err!("{}", err)),
    }
  }
}
impl<T: MethodQuery> MethodQuery for Result<T> {
  type Query = T::Query;
  type ChainedQuery = T::ChainedQuery;

  fn method_query(&self, query: Self::Query) -> Result<()> {
    match self {
      Ok(inner) => inner.method_query(query),
      Err(err) => Err(failure::format_err!("{}", err)),
    }
  }

  fn chained_query(&self, query: Self::ChainedQuery) -> Result<()> {
    match self {
      Ok(inner) => inner.chained_query(query),
      Err(err) => Err(failure::format_err!("{}", err)),
    }
  }
}

impl<T: FieldQuery> FieldQuery for Option<T> {
  type Query = T::Query;

  fn field_query(&self, query: Self::Query) -> Result<()> {
    if let Some(inner) = self {
      inner.field_query(query)?;
    }
    Ok(())
  }
}
impl<T: MethodQuery> MethodQuery for Option<T> {
  type Query = T::Query;
  type ChainedQuery = T::ChainedQuery;

  fn method_query(&self, query: Self::Query) -> Result<()> {
    if let Some(inner) = self {
      inner.method_query(query)?;
    }
    Ok(())
  }

  fn chained_query(&self, query: Self::ChainedQuery) -> Result<()> {
    if let Some(inner) = self {
      inner.chained_query(query)?;
    }
    Ok(())
  }
}
