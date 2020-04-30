use super::State;
use crate::error::Result;
use crate::store::Store;
use std::ops::{Deref, DerefMut};

pub struct Wrapper<S: Store>(S);

impl<S: Store> State<S> for Wrapper<S> {
    fn wrap_store(store: S) -> Result<Wrapper<S>> {
        Ok(Wrapper(store))
    }
}

impl<S: Store> Deref for Wrapper<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &self.0
    }
}

impl<S: Store> DerefMut for Wrapper<S> {
    fn deref_mut(&mut self) -> &mut S {
        &mut self.0
    }
}
