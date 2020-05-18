use crate::{Result, Store};

mod value;
mod wrapper;

pub use value::Value;
pub use wrapper::WrapperStore;

pub trait State<S: Store>: Sized {
    fn wrap_store(store: S) -> Result<Self>;
}
