use crate::Store;

pub mod value;

pub trait WrapStore<'a, Wrapper = Self> {
    fn wrap_store(store: &'a mut dyn Store) -> Wrapper;
}
