use crate::Result;
use super::AsyncQuery;
use futures_lite::future;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

#[must_use]
pub struct PrimitiveClient<T, U: Clone> {
    pub(super) parent: U,
    pub(super) fut: Option<future::Boxed<Result<T>>>,
    pub(super) marker: PhantomData<T>,
}

impl<T, U: Clone> Clone for PrimitiveClient<T, U> {
    fn clone(&self) -> Self {
        PrimitiveClient {
            parent: self.parent.clone(),
            fut: None,
            marker: PhantomData,
        }
    }
}

impl<T, U: Clone + AsyncQuery<Query = (), Response = T>> Future for PrimitiveClient<T, U> {
    type Output = Result<T>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unsafe {
            let this = self.get_unchecked_mut();

            if this.fut.is_none() {
                // make query, populate future to maybe be polled later
                let fut = this.parent.query((), Ok);
                let fut2: future::Boxed<Result<T>> = std::mem::transmute(fut);
                this.fut = Some(fut2);
            }

            let fut = this.fut.as_mut().unwrap().as_mut();
            let res = fut.poll(cx);
            if res.is_ready() {
                this.fut = None;
            }
            res
        }
    }
}
