use std::clone::Clone;
use std::net::ToSocketAddrs;
use std::sync::{Arc, Mutex, MutexGuard};
use error_chain::bail;
use crate::{StateMachine, Store, WriteCache, MapStore, Result, step_atomic};

pub use abci2::messages::abci::{Request, Response};

pub trait ABCIStateMachineFunc: 'static + StateMachine<Request, Response> + Clone + Sized + Sync + Send {
    fn listen<A, S>(&'static self, addr: A, store: S) -> Result<ABCIStateMachine<Self, S>>
        where
            A: ToSocketAddrs,
            S: 'static + Store + Sync + Send
    {
        ABCIStateMachine::new(self, store)
            .listen(addr)
    }
}

impl<S> ABCIStateMachineFunc for S
    where S: 'static + StateMachine<Request, Response> + Clone + Sized + Sync + Send
{}

pub struct ABCIStateMachine<F, S>
    where
        F: ABCIStateMachineFunc,
        S: Store + Sync
{
    func: &'static F,
    store: Arc<Mutex<S>>
}

impl<F, S> ABCIStateMachine<F, S>
    where
        F: 'static + ABCIStateMachineFunc,
        S: 'static + Store + Sync + Send
{
    pub fn new(func: &'static F, store: S) -> Self {
        ABCIStateMachine {
            func,
            store: Arc::new(Mutex::new(store))
        }
    }

    fn from_fields(func: &'static F, store: Arc<Mutex<S>>) -> Self {
        ABCIStateMachine { func, store }
    }

    pub fn run(&mut self, req: Request) -> Result<Response> {
        let mut store = self.store()?;
        step_atomic(&self.func, &mut store, req)
    }

    pub fn store(&self) -> Result<MutexGuard<S>> {
        match self.store.lock() {
            Err(_) => bail!("Could not acquire lock"),
            Ok(store) => Ok(store)
        }
    }

    pub fn listen<A: ToSocketAddrs>(mut self, addr: A) -> Result<Self> {
        let server = abci2::Server::listen(addr)?;

        // TODO: keep workers in struct
        self.create_worker(server.accept()?)?;
        self.create_worker(server.accept()?)?;
        self.create_worker(server.accept()?)?;

        Ok(self)
    }

    fn create_worker(&self, conn: abci2::Connection) -> Result<Worker> {
        Ok(Worker::new(self.func, self.store.clone(), conn))
    }
}

struct Worker {
    thread: std::thread::JoinHandle<()>
    // TODO: keep handle to connection or socket so we can close it
}

impl Worker {
    fn new<F, S>(func: &'static F, store: Arc<Mutex<S>>, conn: abci2::Connection) -> Self
        where
            F: 'static + ABCIStateMachineFunc,
            S: 'static + Store + Sync + Send
    {
        let thread = std::thread::spawn(move || {
            let mut sm = ABCIStateMachine::from_fields(func, store);
            loop {
                // TODO: pass errors through a channel instead of panicking
                let req = conn.read().unwrap();
                let res = sm.run(req).unwrap();
                conn.write(res).unwrap();
            }
        });
        Worker { thread }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn null_abcismf(_: &mut dyn Store, _: Request) -> Result<Response> {
        Ok(Response::new())
    }

    #[test]
    fn gets_abci_impls() {
        fn assert_abcismf(sm: impl ABCIStateMachineFunc) {}
        assert_abcismf(null_abcismf);
    }

    #[test]
    fn simple() {
        let store = MapStore::new();
        null_abcismf.listen("localhost:26658", store).unwrap();
        std::thread::park();
    }
}
