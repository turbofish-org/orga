mod atomic;
mod router;

use crate::error::Result;
use crate::store::Store;

pub use atomic::step_atomic;

// pub trait StateMachine<S: Store, I, O> {
//     fn step(&self, store: S, input: I) -> Result<O>;
// }

// impl<F, S, I, O> StateMachine<S, I, O> for F
//     where
//         S: Store,
//         F: Fn(S, I) -> Result<O>
// {
//     fn step(&self, store: S, input: I) -> Result<O> {
//         self.call((store, input))
//     }
// }

// pub trait Atomic<S: Store, I, O>: StateMachine<S, I, O> {}
