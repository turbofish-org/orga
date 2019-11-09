mod atomic;
mod router;

use crate::error::Result;
use crate::store::Store;

pub use atomic::{step_atomic, atomize};

pub trait StateMachine<S: Store, I, O>: Fn(&mut S, I) -> Result<O> {}
impl<F, S, I, O> StateMachine<S, I, O> for F
    where
        F: Fn(&mut S, I) -> Result<O>,
        S: Store
{}

pub trait Atomic<S: Store, I, O>: StateMachine<S, I, O> {}
