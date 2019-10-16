use crate::error::Result;
use crate::store::{WriteCache, Flush};
use super::{StateMachine, Store, Atomic};

pub struct Atom<T: StateMachine> (T);

impl<T: StateMachine> Atom<T> {
    pub fn new(sm: T) -> Self {
        Atom(sm)
    }
}

impl<T: StateMachine> StateMachine for Atom<T> {
    type Input = T::Input;
    type Output = T::Output;

    fn step<S>(&mut self, action: Self::Input, store: &mut S) -> Result<Self::Output>
        where S: Store
    {
        let mut flush_store = WriteCache::wrap(store);

        match self.0.step(action, &mut flush_store) {
            Err(err) => Err(err),
            Ok(res) => {
                flush_store.finish().flush(store)?;
                Ok(res)
            }
        }
    }
}

impl<T: StateMachine> Atomic for Atom<T> {}
