use crate::error::Result;
use crate::store::Store;

pub trait StateMachine<A> {
    fn step<S>(&mut self, action: A, store: S) -> Result<()>
        where S: Store;
}
