use crate::{Store, Result};

mod value;

pub use value::Value;

pub trait WrapStore<S: Store>: Sized {
    fn wrap_store(store: S) -> Result<Self>;
}
