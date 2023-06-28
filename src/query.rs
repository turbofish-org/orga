use crate::encoding::{Decode, Encode};
use crate::{Error, Result};
use std::error::Error as StdError;
use std::io::Read;
use std::result::Result as StdResult;

pub use orga_macros::{query_block, FieldQuery};

pub const PREFIX_OFFSET: u8 = 0x80;
pub trait Query {
    type Query: Encode + Decode + std::fmt::Debug + Send + Sync;

    fn query(&self, query: Self::Query) -> Result<()>;
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

#[derive(Encode, Decode, Debug)]
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
    T: Query + std::fmt::Debug,
    U: Query + std::fmt::Debug,
{
    type Query = Tuple2Query<T, U>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Tuple2Query::Field0(query) => self.0.query(query),
            Tuple2Query::Field1(query) => self.1.query(query),
        }
    }
}

#[derive(Debug, Encode, Decode)]
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
    T: Query + std::fmt::Debug,
    U: Query + std::fmt::Debug,
    V: Query + std::fmt::Debug,
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

#[derive(Encode, Decode, Debug)]
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
    T: Query + std::fmt::Debug,
    U: Query + std::fmt::Debug,
    V: Query + std::fmt::Debug,
    W: Query + std::fmt::Debug,
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

pub enum Item<T: std::fmt::Debug, U: std::fmt::Debug> {
    Field(T),
    Method(U),
}

impl<T: std::fmt::Debug, U: std::fmt::Debug> std::fmt::Debug for Item<T, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::Field(field) => field.fmt(f),
            Item::Method(method) => method.fmt(f),
        }
    }
}

impl<T: Encode + std::fmt::Debug, U: Encode + std::fmt::Debug> Encode for Item<T, U> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match self {
            Item::Field(field) => {
                field.encode_into(dest)?;
            }
            Item::Method(method) => {
                let mut bytes = method.encode()?;
                if !bytes.is_empty() && bytes[0] < PREFIX_OFFSET {
                    bytes[0] += PREFIX_OFFSET;
                } else {
                    return Err(ed::Error::UnencodableVariant);
                }
                dest.write_all(&bytes)?;
            }
        }

        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        match self {
            Item::Field(field) => field.encoding_length(),
            Item::Method(method) => method.encoding_length(),
        }
    }
}

impl<T: Decode + std::fmt::Debug, U: Decode + std::fmt::Debug> Decode for Item<T, U> {
    fn decode<R: std::io::Read>(input: R) -> ed::Result<Self> {
        let mut input = input;
        let mut buf = [0u8; 1];
        input.read_exact(&mut buf)?;
        let prefix = buf[0];

        if prefix < PREFIX_OFFSET {
            let input = buf.chain(input);
            let field = T::decode(input)?;
            Ok(Item::Field(field))
        } else {
            let bytes = [prefix - PREFIX_OFFSET; 1];
            let input = bytes.chain(input);
            let method = U::decode(input)?;
            Ok(Item::Method(method))
        }
    }
}
pub trait FieldQuery {
    type FieldQuery: Encode + Decode + std::fmt::Debug + Send + Sync = ();

    fn field_query(&self, query: Self::FieldQuery) -> Result<()>;
}

pub trait MethodQuery {
    type MethodQuery: Encode + Decode + std::fmt::Debug + Send + Sync = ();

    fn method_query(&self, query: Self::MethodQuery) -> Result<()>;
}

impl<T> MethodQuery for T {
    default type MethodQuery = ();
    default fn method_query(&self, _query: Self::MethodQuery) -> Result<()> {
        Err(Error::Query("Method not found".to_string()))
    }
}

impl<T> Query for T
where
    T: FieldQuery + MethodQuery,
{
    type Query = Item<T::FieldQuery, T::MethodQuery>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            Item::Field(query) => self.field_query(query),
            Item::Method(query) => self.method_query(query),
        }
    }
}
