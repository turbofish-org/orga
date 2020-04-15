mod atomic;
mod router;

use crate::error::Result;
use crate::store::Store;

pub use atomic::{atomize, step_atomic};

pub trait StateMachine<S: Store, I, O>: Fn(S, I) -> Result<O> {}
impl<S, F, I, O> StateMachine<S, I, O> for F
where
    S: Store,
    F: Fn(S, I) -> Result<O>,
{
}

pub trait Atomic<S: Store, I, O>: StateMachine<S, I, O> {}
