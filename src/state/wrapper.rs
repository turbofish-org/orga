use super::State;
use crate::error::Result;
use crate::store::Store;
use std::ops::{Deref, DerefMut};

pub struct WrapperStore<S: Store>(S);

impl<S: Store> State<S> for WrapperStore<S> {
    fn wrap_store(store: S) -> Result<WrapperStore<S>> {
        Ok(WrapperStore(store))
    }
}

impl<S: Store> Deref for WrapperStore<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &self.0
    }
}

impl<S: Store> DerefMut for WrapperStore<S> {
    fn deref_mut(&mut self) -> &mut S {
        &mut self.0
    }
}
