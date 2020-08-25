use crate::Result;
use crate::store::Read;

pub mod value;
pub mod wrapper;

pub use value::Value;
pub use wrapper::WrapperStore;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
pub trait State<S: Read>: Sized {
    fn wrap_store(store: S) -> Result<Self>;
}

pub trait Query {
    type Request;
    type Response;

    fn query(&self, req: Self::Request) -> Result<Self::Response>;

    fn resolve(&self, req: Self::Request) -> Result<()> {
        self.query(req)?;
        Ok(())
    }
}
