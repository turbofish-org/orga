use crate::{Result, Store};

mod value;
mod wrapper;

pub use value::Value;
pub use wrapper::Wrapper;

pub trait WrapStore<S: Store>: Sized {
    fn wrap_store(store: S) -> Result<Self>;
}
