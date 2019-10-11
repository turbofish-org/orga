use crate::error::Result;
use crate::store::Store;

pub trait StateMachine<A> {
    fn step<S: Store>(&mut self, action: A, store: S) -> Result<()>;
}
