use std::marker::PhantomData;
use crate::error::Result;
use crate::store::{Store, Flush};

pub trait StateMachine<A> {
    fn step<S: Store>(&mut self, action: A, store: &mut S) -> Result<()>;
}


pub trait WrapStore: Store + Flush {
    fn wrap<S: Store>(store: &mut S) -> Self;
} 

pub struct Flusher<A, M, W>
    where
        M: StateMachine<A>,
        W: WrapStore
{
    state_machine: M,
    phantom_a: PhantomData<A>,
    phantom_w: PhantomData<W>
}

impl<A, M, W> Flusher<A, M, W>
    where
        M: StateMachine<A>,
        W: WrapStore
{
    pub fn new(state_machine: M) -> Self {
        Self {
            state_machine,
            phantom_a: PhantomData,
            phantom_w: PhantomData
        }
    }
}

impl<A, M, W> StateMachine<A> for Flusher<A, M, W>
    where
        M: StateMachine<A>,
        W: WrapStore
{
    fn step<S: Store>(&mut self, action: A, store: &mut S) -> Result<()> {
        let mut flush_store = W::wrap(store);

        match self.state_machine.step(action, &mut flush_store) {
            Err(err) => Err(err),
            Ok(_) => flush_store.flush()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::{Error, ErrorKind};
    use crate::store::{ErrorKind::NotFound, MapStore};
    use super::*;

    struct CounterSM ();

    impl StateMachine<u8> for CounterSM {
        fn step<S: Store>(&mut self, n: u8, store: &mut S) -> Result<()> {
            let count = match store.get(b"count") {
                Err(Error(ErrorKind::Store(crate::store::Error(NotFound, _)), _)) => 0,
                Ok(vec) => vec[0],
                Err(err) => return Err(err)
            };

            if count != n {
                return Err("Invalid count".into());
            }

            store.put(b"count".to_vec(), vec![count + 1])?;

            Ok(())
        }
    }

    #[test]
    fn step_counter_error() {
        let mut sm = CounterSM();
        let mut store = MapStore::new();
        assert!(sm.step(100, &mut store).is_err());
    }

    #[test]
    fn step_counter() {
        let mut sm = CounterSM();
        let mut store = MapStore::new();
        assert!(sm.step(0, &mut store).is_ok());
        assert!(sm.step(0, &mut store).is_err());
        assert!(sm.step(1, &mut store).is_ok());
        assert!(sm.step(1, &mut store).is_err());
    }
}
