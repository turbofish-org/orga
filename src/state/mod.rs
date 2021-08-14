use crate::store::*;
use crate::Result;
pub use orga_macros::State;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
///
/// These types can be complex types like collections (e.g. maps), or simple
/// data types (e.g. account structs).
pub trait State<S = DefaultBackingStore>: Sized {
    /// A type which provides the binary encoding of data stored in the type's
    /// root key/value entry. When being written to a store, `State` values will
    /// be converted to this type and then encoded into bytes. When being
    /// loaded, bytes will be decoded as this type and then passed to `create()`
    /// along with access to the store to construct the `State` value.
    ///
    /// For complex types which store all of their data in child key/value
    /// entries, this will often be the unit tuple (`()`) in order to provide a
    /// no-op encoding.
    ///
    /// For simple data types, this will often be `Self` since a separate type
    /// is not needed.
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    /// Creates an instance of the type from a dedicated substore (`store`) and
    /// associated data (`data`).
    ///
    /// Implementations which only represent simple data and do not need access
    /// to the store can just ignore the `store` argument.
    ///
    /// This method will be called by some external container and will rarely be
    /// explicitly called to construct an instance of the type.
    fn create(store: Store<S>, data: Self::Encoding) -> Result<Self>
    where
        S: Read;

    /// Called when the data is to be written to the backing store, and converts
    /// the instance into `Self::Encoding` in order to specify how it should be
    /// represented in binary bytes.
    ///
    /// Note that the type does not write its own binary representation, it is
    /// assumed some external container will store the bytes in a relevant part
    /// of the store. The type does however need to write any child key/value
    /// entries (often by calling child `State` types' `flush()` implementations
    /// then storing their resulting binary representations) to the store if
    /// necessary.
    fn flush(self) -> Result<Self::Encoding>
    where
        S: Write;
}

impl<T: ed::Encode + ed::Decode> State for T {
    type Encoding = Self;

    #[inline]
    fn create(_: Store, value: Self) -> Result<Self> {
        Ok(value)
    }

    #[inline]
    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

/// A trait for state types that can have their data queried by a client.
///
/// A `Query` implementation will typically just call existing getter methods,
/// with the trait acting as a generic way to call these methods.
pub trait Query {
    /// The type of value sent from the client to the node which is resolving
    /// the query.
    type Request;

    /// The type of value returned to the client when a query is successfully
    /// resolved.
    type Response;

    /// Gets data from the state based on the incoming request, and returns it.
    ///
    /// This will be called client-side in order to reproduce the state access
    /// in order for the client to fully verify the data.
    fn query(&self, req: Self::Request) -> Result<Self::Response>;

    /// Accesses the underlying store to get the data necessary for the incoming
    /// query.
    ///
    /// This is called on the resolving node in order to know which raw store
    /// data to send back to the client to let the client successfully call
    /// `query`, using an instrumented store type which records which keys are
    /// accessed.
    ///
    /// The default implementation for `resolve` is to simply call `query` and
    /// throw away the response for ease of implementation, but this will
    /// typically mean unnecessary decoding the result type. Implementations may
    /// override `resolve` to more efficiently query the state without the extra
    /// decode step.
    fn resolve(&self, req: Self::Request) -> Result<()> {
        self.query(req)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct QueryResponder(Result<u64>);

    impl Query for QueryResponder {
        type Request = ();
        type Response = u64;

        fn query(&self, _req: ()) -> Result<u64> {
            match &self.0 {
                Ok(value) => Ok(*value),
                Err(err) => Err(failure::err_msg(err.to_string())),
            }
        }
    }

    #[test]
    fn default_query_resolve_ok() {
        QueryResponder(Ok(42)).resolve(()).unwrap();
    }

    #[test]
    fn default_query_resolve_err() {
        assert_eq!(
            QueryResponder(Err(failure::format_err!("err")))
                .resolve(())
                .unwrap_err()
                .to_string(),
            "err",
        );
    }
}
