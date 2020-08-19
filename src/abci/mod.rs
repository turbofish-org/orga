use std::clone::Clone;
use std::env;
use std::net::ToSocketAddrs;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

use failure::bail;
use log::info;

use crate::state_machine::step_atomic;
use crate::store::{BufStore, BufStoreMap, Flush, MapStore, Read, Store, Write};
use crate::Result;

pub use abci2::messages::abci as messages;
use abci2::messages::abci::Request_oneof_value::*;
use abci2::messages::abci::*;
pub use abci2::messages::abci::{Request, Response};

mod tendermint_client;
pub use tendermint_client::TendermintClient;

/// Top-level struct for running an ABCI application. Maintains an ABCI server,
/// mempool, and handles committing data to the store.
pub struct ABCIStateMachine<A: Application, S: ABCIStore> {
    app: Option<A>,
    store: S,
    receiver: Receiver<(Request, SyncSender<Response>)>,
    sender: SyncSender<(Request, SyncSender<Response>)>,
    mempool_state: Option<BufStoreMap>,
    consensus_state: Option<BufStoreMap>,
    height: u64,
}

impl<A: Application, S: ABCIStore> ABCIStateMachine<A, S> {
    /// Constructs an `ABCIStateMachine` from the given app (a set of handlers
    /// for transactions and blocks), and store (a key/value store to persist
    /// the state data).
    pub fn new(app: A, store: S) -> Self {
        let (sender, receiver) = sync_channel(0);
        ABCIStateMachine {
            app: Some(app),
            store,
            sender,
            receiver,
            mempool_state: Some(Default::default()),
            consensus_state: Some(Default::default()),
            height: 0,
        }
    }

    /// Handles a single incoming ABCI request.
    ///
    /// Some messages, such as `info`, `flush`, and `echo` are automatically
    /// handled by the `ABCIStateMachine`, while others are passed to the
    /// [`Application`](trait.Application.html).
    pub fn run(&mut self, req: Request) -> Result<Response> {
        let value = match req.value {
            None => bail!("Received empty request"),
            Some(value) => value,
        };

        match value {
            info(_) => {
                let mut res = Response::new();
                let mut message = ResponseInfo::new();
                message.set_data("Rust ABCI State Machine".to_string());
                message.set_version("X".to_string());
                message.set_app_version(0);

                let start_height = self.store.height()?;
                info!("State is at height {}", start_height);

                let app_hash = if start_height == 0 {
                    vec![]
                } else {
                    self.store.root_hash()?
                };

                message.set_last_block_height(start_height as i64);
                message.set_last_block_app_hash(app_hash);

                res.set_info(message);
                Ok(res)
            }
            flush(_) => {
                let mut res = Response::new();
                res.set_flush(ResponseFlush::new());
                Ok(res)
            }
            echo(_) => {
                let mut res = Response::new();
                res.set_echo(ResponseEcho::new());
                Ok(res)
            }
            set_option(_) => {
                let mut res = Response::new();
                res.set_set_option(ResponseSetOption::new());
                Ok(res)
            }
            query(req) => {
                // TODO: handle multiple keys (or should this be handled by store impl?)
                let key = req.get_data();
                let data = self.store.query(key)?;

                // TODO: indicate if key doesn't exist vs just being empty
                let mut res = Response::new();
                let mut res_query = ResponseQuery::new();
                res_query.set_code(0);
                res_query.set_index(0);
                res_query.set_log("".to_string());
                res_query.set_value(data);
                res_query.set_height(self.height as i64);
                res.set_query(res_query);
                Ok(res)
            }
            init_chain(req) => {
                let app = self.app.take().unwrap();
                let mut store =
                    BufStore::wrap_with_map(&mut self.store, self.consensus_state.take().unwrap());

                let res_init_chain =
                    match step_atomic(|store, req| app.init_chain(store, req), &mut store, req) {
                        Ok(res) => res,
                        Err(_) => Default::default(),
                    };

                store.flush()?;
                self.store.commit(self.height)?;

                self.app.replace(app);
                self.consensus_state.replace(Default::default());

                let mut res = Response::new();
                res.set_init_chain(res_init_chain);
                Ok(res)
            }
            begin_block(req) => {
                let app = self.app.take().unwrap();
                let mut store =
                    BufStore::wrap_with_map(&mut self.store, self.consensus_state.take().unwrap());

                let res_begin_block = match step_atomic(
                    |store: &mut BufStore<&mut BufStore<&mut S>>, req| app.begin_block(store, req),
                    &mut store,
                    req,
                ) {
                    Ok(res) => res,
                    Err(_) => Default::default(),
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_begin_block(res_begin_block);
                Ok(res)
            }
            deliver_tx(req) => {
                let app = self.app.take().unwrap();
                let mut store =
                    BufStore::wrap_with_map(&mut self.store, self.consensus_state.take().unwrap());

                let res_deliver_tx = match step_atomic(
                    |store: &mut BufStore<&mut BufStore<&mut S>>, req| app.deliver_tx(store, req),
                    &mut store,
                    req,
                ) {
                    Ok(res) => res,
                    Err(err) => {
                        let mut res: ResponseDeliverTx = Default::default();
                        res.set_code(1);
                        res.set_info(format!("{}", err));
                        res
                    }
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_deliver_tx(res_deliver_tx);
                Ok(res)
            }
            end_block(req) => {
                self.height = req.get_height() as u64;

                let app = self.app.take().unwrap();
                let mut store =
                    BufStore::wrap_with_map(&mut self.store, self.consensus_state.take().unwrap());

                let res_end_block = match step_atomic(
                    |store: &mut BufStore<&mut BufStore<&mut S>>, req| app.end_block(store, req),
                    &mut store,
                    req,
                ) {
                    Ok(res) => res,
                    Err(_) => Default::default(),
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                let mut res = Response::new();
                res.set_end_block(res_end_block);
                Ok(res)
            }
            commit(_) => {
                let mut store =
                    BufStore::wrap_with_map(&mut self.store, self.consensus_state.take().unwrap());
                store.flush()?;
                self.store.commit(self.height)?;

                if let Some(stop_height_str) = env::var_os("STOP_HEIGHT") {
                    let stop_height: u64 = stop_height_str
                        .into_string()
                        .unwrap()
                        .parse()
                        .expect("Invalid STOP_HEIGHT value");
                    if self.height >= stop_height {
                        panic!("Reached stop height ({})", stop_height);
                    }
                }

                self.mempool_state.replace(Default::default());
                self.consensus_state.replace(Default::default());

                let mut res = Response::new();
                let mut message = ResponseCommit::new();
                message.set_data(self.store.root_hash()?);
                res.set_commit(message);
                Ok(res)
            }
            check_tx(req) => {
                let app = self.app.take().unwrap();
                let mut store =
                    BufStore::wrap_with_map(&mut self.store, self.mempool_state.take().unwrap());

                let res_check_tx = match step_atomic(
                    |store: &mut BufStore<&mut BufStore<&mut S>>, req| app.check_tx(store, req),
                    &mut store,
                    req,
                ) {
                    Ok(res) => res,
                    Err(err) => {
                        let mut res: ResponseCheckTx = Default::default();
                        res.set_code(1);
                        res.set_info(format!("{}", err));
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

    /// Creates a TCP server for the ABCI protocol and begins handling the
    /// incoming connections.
    pub fn listen<SA: ToSocketAddrs>(mut self, addr: SA) -> Result<()> {
        let server = abci2::Server::listen(addr)?;

        // TODO: keep workers in struct
        // TODO: more intelligently handle connections, e.g. handle tendermint dying/reconnecting?
        self.create_worker(server.accept()?)?;
        self.create_worker(server.accept()?)?;
        self.create_worker(server.accept()?)?;

        loop {
            let (req, cb) = self.receiver.recv().unwrap();
            let res = self.run(req)?;
            cb.send(res).unwrap();
        }
    }

    /// Creates a new worker to handle the incoming ABCI requests for `conn`
    /// within its own threads.
    fn create_worker(&self, conn: abci2::Connection) -> Result<Worker> {
        Ok(Worker::new(self.sender.clone(), conn))
    }
}

struct Worker {
    #[allow(dead_code)]
    thread: std::thread::JoinHandle<()>, // TODO: keep handle to connection or socket so we can close it
}

impl Worker {
    fn new(
        req_sender: SyncSender<(Request, SyncSender<Response>)>,
        conn: abci2::Connection,
    ) -> Self {
        let thread = std::thread::spawn(move || {
            let (res_sender, res_receiver) = sync_channel(0);
            loop {
                // TODO: pass errors through a channel instead of panicking
                let req = conn.read().unwrap();
                req_sender
                    .send((req, res_sender.clone()))
                    .expect("failed to send request");
                let res = res_receiver.recv().unwrap();
                conn.write(res).unwrap();
            }
        });
        Worker { thread }
    }
}

/// An interface for handling ABCI requests.
///
/// All methods have a default implemenation which returns an empty response.
///
/// Only exposes the core state machine requests since messages like Echo and
/// Info are automatically handled within
/// [`ABCIStateMachine`](struct.ABCIStateMachine.html).
pub trait Application {
    fn init_chain<S: Store>(&self, _store: S, _req: RequestInitChain) -> Result<ResponseInitChain> {
        Ok(Default::default())
    }

    fn begin_block<S: Store>(
        &self,
        _store: S,
        _req: RequestBeginBlock,
    ) -> Result<ResponseBeginBlock> {
        Ok(Default::default())
    }

    fn deliver_tx<S: Store>(&self, _store: S, _req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        Ok(Default::default())
    }

    fn end_block<S: Store>(&self, _store: S, _req: RequestEndBlock) -> Result<ResponseEndBlock> {
        Ok(Default::default())
    }

    fn check_tx<S: Store>(&self, _store: S, _req: RequestCheckTx) -> Result<ResponseCheckTx> {
        Ok(Default::default())
    }
}

/// Interface for persisting ABCI app state, as a supertrait of [`store::Store`](../store/trait.Store.html).
pub trait ABCIStore: Store {
    fn height(&self) -> Result<u64>;

    fn root_hash(&self) -> Result<Vec<u8>>;

    fn query(&self, key: &[u8]) -> Result<Vec<u8>>;

    fn commit(&mut self, height: u64) -> Result<()>;
}

/// A basic implementation of [`ABCIStore`](trait.ABCIStore.html) which persists
/// data in memory (mostly for use in testing).
pub struct MemStore {
    height: u64,
    store: MapStore,
}

impl MemStore {
    pub fn new() -> Self {
        MemStore {
            height: 0,
            store: MapStore::new(),
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
            Err(e) => Err(e),
        }
    }

    fn commit(&mut self, height: u64) -> Result<()> {
        self.height = height;
        Ok(())
    }
}
