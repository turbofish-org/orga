use crate::Store;

pub mod value;

pub trait WrapStore<'a> {
    fn wrap_store<S: Store + 'a>(store: S) -> Self;
}
