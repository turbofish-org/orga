use crate::encoding::{Encode, Decode};
use crate::Result;

pub use orga_macros::{Query, query};

pub trait Query {
  type Query: Encode + Decode;

  fn query(&self, query: Self::Query) -> Result<()>;
}

impl<T: Query> Query for Result<T> {
  type Query = T::Query;

  fn query(&self, query: Self::Query) -> Result<()> {
    match self {
      Ok(inner) => inner.query(query),
      Err(err) => Err(failure::format_err!("{}", err)),
    }
  }
}

impl<T: Query> Query for Option<T> {
  type Query = T::Query;

  fn query(&self, query: Self::Query) -> Result<()> {
    if let Some(inner) = self {
      inner.query(query)?;
    }
    Ok(())
  }
}

// TODO: primitives

impl Query for bool {
  type Query = ();

  fn query(&self, _: Self::Query) -> Result<()> {
    Ok(())
  }
}

impl Query for u32 {
  type Query = ();

  fn query(&self, _: Self::Query) -> Result<()> {
    Ok(())
  }
}

impl Query for () {
  type Query = ();

  fn query(&self, _: Self::Query) -> Result<()> {
    Ok(())
  }
}
