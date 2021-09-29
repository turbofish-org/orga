use std::future::Future;
use std::pin::Pin;
use futures_lite::future;
use super::AsyncCall;
use crate::Result;

#[must_use]
pub struct CallChain<T: Clone, U: Clone + AsyncCall>
where
    U::Call: Default,
{
    wrapped: T,
    parent: U,
    fut: Option<future::Boxed<Result<()>>>,
}

impl<T: Clone, U: Clone + AsyncCall> CallChain<T, U>
where
    U::Call: Default,
{
    pub fn new(wrapped: T, parent: U) -> Self {
        Self {
            wrapped,
            parent,
            fut: None,
        }
    }
}

impl<T: Clone, U: Clone + AsyncCall> Clone for CallChain<T, U>
where
    U::Call: Default,
{
    fn clone(&self) -> Self {
        CallChain {
            wrapped: self.wrapped.clone(),
            parent: self.parent.clone(),
            fut: None,
        }
    }
}

impl<T: Clone, U: Clone + AsyncCall> Future for CallChain<T, U>
where
    U::Call: Default,
{
    type Output = Result<()>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unsafe {
            let this = self.get_unchecked_mut();

            if this.fut.is_none() {
                // make call, populate future to maybe be polled later
                let fut = this.parent.call(Default::default());
                let fut2: future::Boxed<Result<()>> = std::mem::transmute(fut);
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

impl<T: Clone, U: Clone + AsyncCall> std::ops::Deref for CallChain<T, U>
where
    U::Call: Default,
{
    type Target = T;

    fn deref(&self) -> &T {
        &self.wrapped
    }
}

impl<T: Clone, U: Clone + AsyncCall> std::ops::DerefMut for CallChain<T, U>
where
    U::Call: Default,
{
    fn deref_mut(&mut self) -> &mut T {
        &mut self.wrapped
    }
}
