use std::clone::Clone;
use std::net::ToSocketAddrs;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use error_chain::bail;
use crate::{StateMachine, Read, Write, Store, Flush, WriteCache, MapStore, Result, step_atomic, WriteCacheMap};

pub use abci2::messages::abci::{Request, Response};
use abci2::messages::abci::*;
use abci2::messages::abci::Request_oneof_value::*;

pub struct ABCIStateMachine<A: Application, S: ABCIStore> {
    app: Option<A>,
    store: S,
    receiver: Receiver<(Request, SyncSender<Response>)>,
    sender: SyncSender<(Request, SyncSender<Response>)>,
    mempool_state: Option<WriteCacheMap>,
    consensus_state: Option<WriteCacheMap>,
    height: u64
}

impl<A: Application, S: ABCIStore> ABCIStateMachine<A, S> {
    pub fn new(app: A, store: S) -> Self {
        let (sender, receiver) = sync_channel(0);
        ABCIStateMachine {
            app: Some(app),
            store,
            sender,
            receiver,
            mempool_state: Some(Default::default()),
            consensus_state: Some(Default::default()),
            height: 0
        }
    }

    pub fn run(&mut self, req: Request) -> Result<Response> {
        let value = match req.value {
            None => bail!("Received empty request"),
            Some(value) => value
        };

        match value {
            info(_) => {
                let mut res = Response::new();
                let mut message = ResponseInfo::new();
                message.set_data("Rust ABCI State Machine".to_string());
                message.set_version("X".to_string());
                message.set_app_version(0);

                let start_height = self.store.height()?;
                let app_hash = if start_height == 0 {
                    vec![]
                } else {
                    self.store.root_hash()?
                };
                message.set_last_block_height(start_height as i64);
                message.set_last_block_app_hash(app_hash);

                res.set_info(message);
                Ok(res)
            },
            flush(_) => {
                let mut res = Response::new();
                res.set_flush(ResponseFlush::new());
                Ok(res)
            },
            echo(_) => {
                let mut res = Response::new();
                res.set_echo(ResponseEcho::new());
                Ok(res)
            },
            set_option(_) => {
                let mut res = Response::new();
                res.set_set_option(ResponseSetOption::new());
                Ok(res)
            },
            query(req) => {
                // TODO: handle multiple keys (or should this be handled by store impl?)
                let key = req.get_data();

                let data = self.store.query(key)?;

                let mut res = Response::new();
                let mut res_query = ResponseQuery::new();
                res_query.set_value(data);
                res.set_query(res_query);
                Ok(res)
            },
            init_chain(req) => {
                let app = self.app.take().unwrap();
                let mut store = WriteCache::wrap_with_map(
                    &mut self.store,
                    self.consensus_state.take().unwrap()
                );

                let res_init_chain = match step_atomic(|store, req| app.init_chain(store, req), &mut store, req) {
                    Ok(res) => res,
                    Err(_) => Default::default()
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_init_chain(res_init_chain);
                Ok(res)
            },
            begin_block(req) => {
                let app = self.app.take().unwrap();
                let mut store = WriteCache::wrap_with_map(
                    &mut self.store,
                    self.consensus_state.take().unwrap()
                );

                let res_begin_block = match step_atomic(|store, req| app.begin_block(store, req), &mut store, req) {
                    Ok(res) => res,
                    Err(_) => Default::default()
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_begin_block(res_begin_block);
                Ok(res)
            },
            deliver_tx(req) => {
                let app = self.app.take().unwrap();
                let mut store = WriteCache::wrap_with_map(
                    &mut self.store,
                    self.consensus_state.take().unwrap()
                );

                let res_deliver_tx = match step_atomic(|store, req| app.deliver_tx(store, req), &mut store, req) {
                    Ok(res) => res,
                    Err(err) => {
                        let mut res: ResponseDeliverTx = Default::default();
                        res.set_code(1);
                        res.set_info(err.description().to_string());
                        res
                    }
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_deliver_tx(res_deliver_tx);
                Ok(res)
            },
            end_block(req) => {
                self.height = req.get_height() as u64;

                let app = self.app.take().unwrap();
                let mut store = WriteCache::wrap_with_map(
                    &mut self.store,
                    self.consensus_state.take().unwrap()
                );

                let res_end_block = match step_atomic(|store, req| app.end_block(store, req), &mut store, req) {
                    Ok(res) => res,
                    Err(_) => Default::default()
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_end_block(res_end_block);
                Ok(res)
            },
            commit(_) => {
                let mut store = WriteCache::wrap_with_map(
                    &mut self.store,
                    self.consensus_state.take().unwrap()
                );
                store.flush()?;
                self.store.commit(self.height)?;

                self.mempool_state.replace(Default::default());
                self.consensus_state.replace(Default::default());

                let mut res = Response::new();
                let mut message = ResponseCommit::new();
                message.set_data(self.store.root_hash()?);
                res.set_commit(message);
                Ok(res)
            },
            check_tx(req) => {
                let app = self.app.take().unwrap();
                let mut store = WriteCache::wrap_with_map(
                    &mut self.store,
                    self.mempool_state.take().unwrap()
                );

                let res_check_tx = match step_atomic(|store, req| app.check_tx(store, req), &mut store, req) {
                    Ok(res) => res,
                    Err(err) => {
                        let mut res: ResponseCheckTx = Default::default();
                        res.set_code(1);
                        res.set_info(err.description().to_string());
                        res
                    }
                };

                self.app.replace(app);
                self.mempool_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_check_tx(res_check_tx);
                Ok(res)
            }
        }
    }

    pub fn listen<SA: ToSocketAddrs>(mut self, addr: SA) -> Result<()> {
        let server = abci2::Server::listen(addr)?;

        // TODO: keep workers in struct
        self.create_worker(server.accept()?)?;
        self.create_worker(server.accept()?)?;
        self.create_worker(server.accept()?)?;

        loop {
            let (req, cb) = self.receiver.recv().unwrap();
            let res = self.run(req)?;
            cb.send(res).unwrap();   
        }

        Ok(())
    }

    fn create_worker(&self, conn: abci2::Connection) -> Result<Worker> {
        Ok(Worker::new(self.sender.clone(), conn))
    }
}

struct Worker {
    thread: std::thread::JoinHandle<()>
    // TODO: keep handle to connection or socket so we can close it
}

impl Worker {
    fn new(
        req_sender: SyncSender<(Request, SyncSender<Response>)>,
        conn: abci2::Connection
    ) -> Self {
        let thread = std::thread::spawn(move || {
            let (res_sender, res_receiver) = sync_channel(0);
            loop {
                // TODO: pass errors through a channel instead of panicking
                let req = conn.read().unwrap();
                req_sender.send((req, res_sender.clone()))
                    .expect("failed to send request");
                let res = res_receiver.recv().unwrap();
                conn.write(res).unwrap();
            }
        });
        Worker { thread }
    }
}

pub trait Application {
    fn init_chain(&self, store: &mut dyn Store, req: RequestInitChain) -> Result<ResponseInitChain> {
        Ok(Default::default())
    }

    fn begin_block(&self, store: &mut dyn Store, req: RequestBeginBlock) -> Result<ResponseBeginBlock> {
        Ok(Default::default())
    }

    fn deliver_tx(&self, store: &mut dyn Store, req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        Ok(Default::default())
    }

    fn end_block(&self, store: &mut dyn Store, req: RequestEndBlock) -> Result<ResponseEndBlock> {
        Ok(Default::default())
    }

    fn check_tx(&self, store: &mut dyn Store, req: RequestCheckTx) -> Result<ResponseCheckTx> {
        Ok(Default::default())
    }
}

pub trait ABCIStore: Store {
    fn height(&self) -> Result<u64>;

    fn root_hash(&self) -> Result<Vec<u8>>;

    fn query(&self, key: &[u8]) -> Result<Vec<u8>>;

    fn commit(&mut self, height: u64) -> Result<()>;
}

pub struct MemStore {
    height: u64,
    store: MapStore
}

impl MemStore {
    pub fn new() -> Self {
        MemStore {
            height: 0,
            store: MapStore::new()
        }
    }
}

impl Read for MemStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.store.get(key)
    }
}

impl Write for MemStore {
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.store.put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.store.delete(key)
    }
}

impl ABCIStore for MemStore {
    fn height(&self) -> Result<u64> {
        Ok(self.height)
    }

    fn root_hash(&self) -> Result<Vec<u8>> {
        // TODO: real hashing based on writes
        Ok(vec![])
    }

    fn query(&self, key: &[u8]) -> Result<Vec<u8>> {
        match self.get(key) {
            Ok(Some(val)) => Ok(val),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(e)
        }
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        self.height = height;
        Ok(())
    }
}
