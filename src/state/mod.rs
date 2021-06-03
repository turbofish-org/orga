use crate::store::*;
use crate::Result;

pub mod wrapper;

pub use wrapper::WrapperStore;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
pub trait State<S>: Sized {
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    fn create(store: Store<S>, decoded: Self::Encoding) -> Result<Self>
    where
        S: Read;

    fn flush(self) -> Result<Self::Encoding>
    where
        S: Write;
}

impl<S, T: ed::Encode + ed::Decode> State<S> for T {
    type Encoding = Self;

    #[inline]
    fn create(_: Store<S>, value: Self) -> Result<Self> {
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
