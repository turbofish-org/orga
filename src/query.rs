//! Efficient data querying via network messages.

use crate::encoding::{Decode, Encode};
use crate::{Error, Result};
use std::error::Error as StdError;
use std::io::Read;
use std::result::Result as StdResult;

pub use orga_macros::{query_block, FieldQuery};

/// The prefix offset for method queries, added to avoid conflicts with state or
/// method call prefixes for fields.
pub const PREFIX_OFFSET: u8 = 0x80;

/// A trait for efficiently querying data, e.g. by a client from a remote node.
///
/// Query allows the implementor to create a minimal expression of what data is
/// required, typically as a hierarchical combination of [FieldQuery] and
/// [MethodQuery], composed via [Item].
///
/// The [FieldQuery] macro generates variants for each public
/// field on structs, and a type's [MethodQuery] is generated via
/// public methods tagged `#[query]` by the [query_block] macro on an impl block
/// for that type.
///
/// `Query` may also be implemented manually to enable more complex behavior,
/// such as in [crate::plugins::QueryPlugin].
pub trait Query {
    /// The encodable message for queries to this type.
    type Query: Encode + Decode + std::fmt::Debug + Send + Sync;

    /// Perform the query.
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

/// Represents either a field or method query item.
///
/// The encoding of this type handles the prefix byte convention for fields vs.
/// methods.
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

/// A trait for complex types whose fields may also be [Query].
///
/// The [FieldQuery] derive macro generates variants for each field tagged
/// `#[query]`, and the unwrapped query is passed along to that field's [Query]
/// implementation.
///
/// The encoding of the `FieldQuery` uses the field's [State] prefix by default.
pub trait FieldQuery {
    /// The encodable message type for queries to this type's fields.
    type FieldQuery: Encode + Decode + std::fmt::Debug + Send + Sync = ();

    /// Perform the field query.
    fn field_query(&self, query: Self::FieldQuery) -> Result<()>;
}

/// A trait for types that expose public methods as queries (via the `#[query]`
/// attribute and `#[query_block]` macro).
///
/// Method queries are assigned an incrementing byte prefix, starting from
/// [PREFIX_OFFSET] to avoid conflicts with fields or method calls.
///
/// After the prefix byte, the remaining bytes of the encoded method query are
/// the encoded method arguments (which must each implement [Encode] and
/// [Decode]).
pub trait MethodQuery {
    /// The encodable message type for queries to this type's methods.
    type MethodQuery: Encode + Decode + std::fmt::Debug + Send + Sync = ();

    /// Perform the method query.
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
