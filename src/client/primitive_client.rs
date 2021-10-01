use std::marker::PhantomData;

#[must_use]
pub struct PrimitiveClient<T, U: Clone> {
    pub(super) parent: U,
    pub(super) marker: PhantomData<T>,
}

impl<T, U: Clone> PrimitiveClient<T, U> {
    pub fn new(parent: U) -> Self {
        Self {
            parent,
            marker: PhantomData,
        }
    }
}

impl<T, U: Clone> Clone for PrimitiveClient<T, U> {
    fn clone(&self) -> Self {
        PrimitiveClient {
            parent: self.parent.clone(),
            marker: PhantomData,
        }
    }
}
