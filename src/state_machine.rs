use crate::error::Result;
use crate::store::{Store, Flush};

pub trait StateMachine<A> {
    fn step<S: Store>(&mut self, action: A, store: &mut S) -> Result<()>;
}

pub struct Flusher<M, C>
    where
        M: StateMachine<A>,
        C: Fn(&mut Store) -> Flush + Sized
{
    state_machine: M,
    create_store: C
}

impl<A, M, C> StateMachine<A> for Flusher<M, C>
    where
        M: StateMachine<A>,
        C: Fn(&mut Store) -> Flush + Sized
{
    fn step<S: Store>(&mut self, action: A, store: S) -> Result<()> {
        let flush_store = (self.create_store)(store);

        match self.state_machine.step(action, flush_store) {
            Err(err) => Err(err),
            Ok(_) => Ok(flush_store.flush())
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
