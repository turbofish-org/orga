use std::clone::Clone;
use std::env;
use std::net::ToSocketAddrs;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

use failure::bail;
use log::info;

use crate::state_machine::step_atomic;
use crate::store::{BufStore, BufStoreMap, Iter, MapStore, Read, Store, Write, KV};
use crate::Result;

use messages::*;
pub use tendermint_proto::abci as messages;
use tendermint_proto::abci::request::Value as Req;
use tendermint_proto::abci::response::Value as Res;

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
        let (sender, receiver): (
            SyncSender<(Request, SyncSender<Response>)>,
            Receiver<(Request, SyncSender<Response>)>,
        ) = sync_channel(0);
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
    pub fn run(&mut self, req: Request) -> Result<Res> {
        let value = match req.value {
            None => bail!("Received empty request"),
            Some(value) => value,
        };

        match value {
            Req::Info(_) => {
                let mut res_info = ResponseInfo::default();
                res_info.data = "Rust ABCI State Machine".into();
                res_info.version = "X".into();
                res_info.app_version = 0;

                let start_height = self.store.height()?;
                info!("State is at height {}", start_height);

                let app_hash = if start_height == 0 {
                    vec![]
                } else {
                    self.store.root_hash()?
                };

                res_info.last_block_height = start_height as i64;
                res_info.last_block_app_hash = app_hash;

                Ok(Res::Info(res_info))
            }
            Req::Flush(_) => Ok(Res::Flush(Default::default())),
            Req::Echo(_) => Ok(Res::Echo(Default::default())),
            Req::SetOption(_) => Ok(Res::SetOption(Default::default())),
            Req::Query(req) => {
                todo!()
                // // TODO: handle multiple keys (or should this be handled by store impl?)
                // let key = req.data;
                // let data = self.store.query(&key)?;

                // // TODO: indicate if key doesn't exist vs just being empty
                // let mut res = ResponseQuery::default();
                // res.code = 0;
                // res.index = 0;
                // res.value = data;
                // res.height = self.height as i64;
                // Ok(Res::Query(res))
            }
            Req::InitChain(req) => {
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

                Ok(Res::InitChain(res_init_chain))
            }
            Req::BeginBlock(req) => {
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

                Ok(Res::BeginBlock(res_begin_block))
            }
            Req::DeliverTx(req) => {
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
                        res.code = 1;
                        res.log = format!("{}", err);
                        res
                    }
                };

                self.app.replace(app);
                self.consensus_state.replace(store.into_map());

                Ok(Res::DeliverTx(res_deliver_tx))
            }
            Req::EndBlock(req) => {
                self.height = req.height as u64;

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

                Ok(Res::EndBlock(res_end_block))
            }
            Req::Commit(_) => {
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

                let mut res_commit = ResponseCommit::default();
                res_commit.data = self.store.root_hash()?;
                Ok(Res::Commit(res_commit))
            }
            Req::CheckTx(req) => {
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
                        res.code = 1;
                        res.log = format!("{}", err);
                        res
                    }
                };

                self.app.replace(app);
                self.mempool_state.replace(store.into_map());

                Ok(Res::CheckTx(res_check_tx))
            }
            // TODO: state sync
            Req::ListSnapshots(_) => Ok(Res::ListSnapshots(Default::default())),
            Req::OfferSnapshot(_) => Ok(Res::OfferSnapshot(Default::default())),
            Req::LoadSnapshotChunk(_) => Ok(Res::LoadSnapshotChunk(Default::default())),
            Req::ApplySnapshotChunk(_) => Ok(Res::ApplySnapshotChunk(Default::default())),
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
        self.create_worker(server.accept()?)?;

        loop {
            let (req, cb) = self.receiver.recv().unwrap();
            let res = Response {
                value: Some(self.run(req)?),
            };
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
    fn init_chain<S>(&self, _store: S, _req: RequestInitChain) -> Result<ResponseInitChain> {
        Ok(Default::default())
    }

    fn begin_block<S>(&self, _store: S, _req: RequestBeginBlock) -> Result<ResponseBeginBlock> {
        Ok(Default::default())
    }

    fn deliver_tx<S>(&self, _store: S, _req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        Ok(Default::default())
    }

    fn end_block<S>(&self, _store: S, _req: RequestEndBlock) -> Result<ResponseEndBlock> {
        Ok(Default::default())
    }

    fn check_tx<S>(&self, _store: S, _req: RequestCheckTx) -> Result<ResponseCheckTx> {
        Ok(Default::default())
    }
}

/// Interface for persisting ABCI app state, as a supertrait of [`store::Store`](../store/trait.Store.html).
pub trait ABCIStore: Read + Write {
    fn height(&self) -> Result<u64>;

    fn root_hash(&self) -> Result<Vec<u8>>;

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

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        self.store.get_next(key)
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

    fn commit(&mut self, height: u64) -> Result<()> {
        self.height = height;
        Ok(())
    }
}
