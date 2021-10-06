use std::marker::PhantomData;

use super::{ABCIStateMachine, ABCIStore, App, Application, WrappedMerk};
use crate::call::Call;
use crate::contexts::{ABCICall, ABCIProvider};
use crate::encoding::{Decode, Encode};
use crate::merk::{BackingStore, MerkStore};
use crate::query::Query;
use crate::state::State;
use crate::store::{Read, Shared, Store, Write};
use crate::tendermint::Tendermint;
use crate::Result;
use std::path::{Path, PathBuf};
use std::borrow::Borrow;
use tendermint_proto::abci::*;
pub struct Node<A> {
    _app: PhantomData<A>,
    tm_home: PathBuf,
    merk_home: PathBuf,
    p2p_port: u16,
    rpc_port: u16,
    abci_port: u16,
    genesis_path: Option<PathBuf>,
    p2p_persistent_peers: Option<Vec<String>>,
}

impl<A: App> Node<A>
where
    <A as State>::Encoding: Default,
{
    pub fn new<P: AsRef<Path>>(home: P) -> Self {
        let home: PathBuf = home.as_ref().into();
        let merk_home = home.join("merk");
        let tm_home = home.join("tendermint");
        if !home.exists() {
            std::fs::create_dir(&home).expect("Failed to initialize application home directory");
        }
        Tendermint::new(tm_home.clone())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .init();

        Node {
            _app: PhantomData,
            merk_home,
            tm_home,
            p2p_port: 26656,
            rpc_port: 26657,
            abci_port: 26658,
            genesis_path: None,
            p2p_persistent_peers: None,
        }
    }

    pub fn run(self) {
        // Start tendermint process
        let tm_home = self.tm_home.clone();
        let p2p_port = self.p2p_port;
        let rpc_port = self.rpc_port;
        let maybe_genesis_path = self.genesis_path;
        let maybe_peers = self.p2p_persistent_peers;
        std::thread::spawn(move || {
            let mut tm_process = Tendermint::new(&tm_home)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .p2p_laddr(format!("tcp://0.0.0.0:{}", p2p_port).as_str())
                .rpc_laddr(format!("tcp://0.0.0.0:{}", rpc_port).as_str()); // Note: public by default

            if let Some(genesis_path) = maybe_genesis_path {
                tm_process = tm_process.with_genesis(genesis_path);
            }

            if let Some(peers) = maybe_peers {
                tm_process = tm_process.p2p_persistent_peers(peers);
            }

            tm_process.start();
        });
        let app = InternalApp::<ABCIProvider<A>>::new();
        let store = MerkStore::new(self.merk_home.clone());

        // Start ABCI server
        ABCIStateMachine::new(app, store)
            .listen(format!("127.0.0.1:{}", self.abci_port))
            .expect("Failed to start ABCI server");
    }

    pub fn reset(self) -> Self {
        if self.merk_home.exists() {
            std::fs::remove_dir_all(&self.merk_home).expect("Failed to clear Merk data");
        }

        Tendermint::new(&self.tm_home)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .unsafe_reset_all();

        self
    }

    pub fn rpc_port(mut self, port: u16) -> Self {
        self.rpc_port = port;

        self
    }

    pub fn p2p_port(mut self, port: u16) -> Self {
        self.p2p_port = port;

        self
    }

    pub fn abci_port(mut self, port: u16) -> Self {
        self.abci_port = port;

        self
    }

    pub fn with_genesis<P: AsRef<Path>>(mut self, genesis_path: P) -> Self {
        self.genesis_path.replace(genesis_path.as_ref().into());

        self
    }

    pub fn peers<T: Borrow<str>>(mut self, peers: &[T]) -> Self {
        let peers = peers.iter().map(|p| p.borrow().to_string()).collect();
        self.p2p_persistent_peers.replace(peers);

        self
    }
}

impl<A> InternalApp<ABCIProvider<A>>
where
    A: App,
    <A as State>::Encoding: Default,
{
    fn run<T, F: FnOnce(&mut ABCIProvider<A>) -> T>(&self, store: WrappedMerk, op: F) -> Result<T> {
        let mut store = Store::new(store.into());
        let state_bytes = match store.get(&[])? {
            Some(inner) => inner,
            None => {
                let default_encoding: A::Encoding = Default::default();
                let encoded_bytes = Encode::encode(&default_encoding).unwrap();
                store.put(vec![], encoded_bytes.clone())?;
                encoded_bytes
            }
        };
        let data: <ABCIProvider<A> as State>::Encoding = Decode::decode(state_bytes.as_slice())?;
        let mut state = <ABCIProvider<A> as State>::create(store.clone(), data)?;
        let res = op(&mut state);
        let flushed = state.flush()?;
        store.put(vec![], flushed.encode()?)?;

        Ok(res)
    }
}

impl<A> Application for InternalApp<ABCIProvider<A>>
where
    A: App,
    <A as State>::Encoding: Default,
{
    fn init_chain(&self, store: WrappedMerk, req: RequestInitChain) -> Result<ResponseInitChain> {
        self.run(store, move |state| state.call(req.into()))??;

        Ok(Default::default())
    }

    fn begin_block(
        &self,
        store: WrappedMerk,
        req: RequestBeginBlock,
    ) -> Result<ResponseBeginBlock> {
        self.run(store, move |state| state.call(req.into()))??;

        Ok(Default::default())
    }

    fn end_block(&self, store: WrappedMerk, req: RequestEndBlock) -> Result<ResponseEndBlock> {
        let mut updates = self.run(store, move |state| -> Result<_> {
            state.call(req.into())?;
            Ok(state
                .validator_updates
                .take()
                .expect("ABCI Provider did not create validator update map"))
        })??;

        // Write back validator updates
        let mut res: ResponseEndBlock = Default::default();
        updates.drain().for_each(|(_key, update)| {
            res.validator_updates.push(update);
        });

        Ok(res)
    }

    fn deliver_tx(&self, store: WrappedMerk, req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        let run_res = self.run(store, move |state| -> Result<_> {
            let inner_call = Decode::decode(req.tx.as_slice())?;
            state.call(ABCICall::DeliverTx(inner_call))
        })?;

        let mut deliver_tx_res = ResponseDeliverTx::default();
        if let Err(err) = run_res {
            deliver_tx_res.code = 1;
            deliver_tx_res.log = err.to_string();
        }

        Ok(deliver_tx_res)
    }

    fn check_tx(&self, store: WrappedMerk, req: RequestCheckTx) -> Result<ResponseCheckTx> {
        let run_res = self.run(store, move |state| -> Result<_> {
            let inner_call = Decode::decode(req.tx.as_slice())?;
            state.call(ABCICall::CheckTx(inner_call))
        })?;
        let mut check_tx_res = ResponseCheckTx::default();
        if let Err(err) = run_res {
            check_tx_res.code = 1;
            check_tx_res.log = err.to_string();
        }

        Ok(check_tx_res)
    }

    fn query(&self, merk_store: Shared<MerkStore>, req: RequestQuery) -> Result<ResponseQuery> {
        let query_bytes = req.data;
        let backing_store: BackingStore = merk_store.clone().into();
        let store_height = merk_store.borrow().height()?;
        let store = Store::new(backing_store.clone());
        let state_bytes = store.get(&[])?.unwrap();
        let data: <ABCIProvider<A> as State>::Encoding = Decode::decode(state_bytes.as_slice())?;
        let state = <ABCIProvider<A> as State>::create(store, data)?;

        // Check which keys are accessed by the query and build a proof
        let query = Decode::decode(query_bytes.as_slice())?;
        state.query(query)?;
        let proof_builder = backing_store.into_proof_builder()?;
        let root_hash = merk_store.borrow().root_hash()?;
        let proof_bytes = proof_builder.build()?;

        // TODO: we shouldn't need to include the root hash in the response
        let mut value = vec![];
        value.extend(root_hash);
        value.extend(proof_bytes);

        let res = ResponseQuery {
            code: 0,
            height: store_height as i64,
            value,
            ..Default::default()
        };

        Ok(res)
    }
}

struct InternalApp<A> {
    _app: PhantomData<A>,
}

impl<A: App> InternalApp<ABCIProvider<A>>
where
    <A as State>::Encoding: Default,
{
    pub fn new() -> Self {
        Self { _app: PhantomData }
    }
}
