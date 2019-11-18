mod atomic;
mod router;

use crate::error::Result;
use crate::store::Store;

pub use atomic::{step_atomic, atomize};
pub use router::Router;
pub use router::Transaction as RouterTransaction;

pub trait StateMachine<I, O>: Fn(&mut dyn Store, I) -> Result<O> {}
impl<F, I, O> StateMachine<I, O> for F
    where F: Fn(&mut dyn Store, I) -> Result<O>
{}

pub trait Atomic<I, O>: StateMachine<I, O> {}
