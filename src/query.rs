use crate::encoding::{Decode, Encode};
use crate::{Error, Result};
use std::error::Error as StdError;
use std::result::Result as StdResult;

pub use orga_macros::{query, Query};

pub trait Query {
    type Query: Encode + Decode + std::fmt::Debug;

    fn query(&self, query: Self::Query) -> Result<()>;
}

impl<T: Query> Query for &T {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        (*self).query(query)
    }
}

impl<T: Query, E: StdError> Query for StdResult<T, E> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        match self {
            Ok(inner) => inner.query(query),
            Err(err) => Err(Error::Query(err.to_string())),
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

macro_rules! noop_impl {
    ($type:ty) => {
        impl Query for $type {
            type Query = ();

            fn query(&self, _: ()) -> Result<()> {
                Ok(())
            }
        }
    };
}

noop_impl!(());
noop_impl!(bool);
noop_impl!(u8);
noop_impl!(u16);
noop_impl!(u32);
noop_impl!(u64);
noop_impl!(u128);
noop_impl!(i8);
noop_impl!(i16);
noop_impl!(i32);
noop_impl!(i64);
noop_impl!(i128);

impl<T> Query for Vec<T> {
    type Query = ();

    fn query(&self, _: ()) -> Result<()> {
        Ok(())
    }
}

impl<T> Query for (T,)
where
    T: Query,
{
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.0.query(query)
    }
}

#[derive(Encode, Decode)]
pub enum Tuple2Query<T, U>
where
    T: Query,
    U: Query,
{
    Field0(T::Query),
    Field1(U::Query),
}

impl<T, U> Query for (T, U)
where
    T: Query,
    U: Query,
{
    type Query = Tuple2Query<T, U>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Tuple2Query::Field0(query) => self.0.query(query),
            Tuple2Query::Field1(query) => self.1.query(query),
        }
    }
}

#[derive(Encode, Decode)]
pub enum Tuple3Query<T, U, V>
where
    T: Query,
    U: Query,
    V: Query,
{
    Field0(T::Query),
    Field1(U::Query),
    Field2(V::Query),
}

impl<T, U, V> Query for (T, U, V)
where
    T: Query,
    U: Query,
    V: Query,
{
    type Query = Tuple3Query<T, U, V>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Tuple3Query::Field0(query) => self.0.query(query),
            Tuple3Query::Field1(query) => self.1.query(query),
            Tuple3Query::Field2(query) => self.2.query(query),
        }
    }
}

#[derive(Encode, Decode)]
pub enum Tuple4Query<T, U, V, W>
where
    T: Query,
    U: Query,
    V: Query,
    W: Query,
{
    Field0(T::Query),
    Field1(U::Query),
    Field2(V::Query),
    Field3(W::Query),
}

impl<T, U, V, W> Query for (T, U, V, W)
where
    T: Query,
    U: Query,
    V: Query,
    W: Query,
{
    type Query = Tuple4Query<T, U, V, W>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Tuple4Query::Field0(query) => self.0.query(query),
            Tuple4Query::Field1(query) => self.1.query(query),
            Tuple4Query::Field2(query) => self.2.query(query),
            Tuple4Query::Field3(query) => self.3.query(query),
        }
    }
}

impl<T: Query, const N: usize> Query for [T; N] {
    type Query = (u64, T::Query);

    fn query(&self, query: Self::Query) -> Result<()> {
        let (index, subquery) = query;
        let index = index as usize;

        if index >= N {
            return Err(Error::Query("Query index out of bounds".into()));
        }

        self[index].query(subquery)
    }
}
