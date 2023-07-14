use super::{ABCIStateMachine, ABCIStore, AbciQuery, App, Application, WrappedMerk};
use crate::call::Call;
use crate::context::Context;
use crate::encoding::Decode;
use crate::merk::{MerkStore, ProofBuilder};
use crate::migrate::Migrate;
use crate::plugins::{ABCICall, ABCIPlugin};
use crate::query::Query;
use crate::state::State;
use crate::store::{BackingStore, Read, Shared, Store, Write};
use crate::tendermint::Tendermint;
use crate::Result;
use home::home_dir;
use std::borrow::Borrow;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tendermint_proto::v0_34::abci::*;

pub struct Node<A> {
    _app: PhantomData<A>,
    tm_home: PathBuf,
    merk_home: PathBuf,
    home: PathBuf,
    abci_port: u16,
    genesis_bytes: Option<Vec<u8>>,
    p2p_persistent_peers: Option<Vec<String>>,
    stdout: Stdio,
    stderr: Stdio,
    logs: bool,
    skip_init_chain: bool,
    flags: Vec<String>,
}

impl Node<()> {
    pub fn home(name: &str) -> PathBuf {
        match std::env::var("NOMIC_HOME_DIR") {
            Ok(home) => Some(PathBuf::from(home)),
            Err(_) => home_dir(),
        }
        .expect("Could not resolve user home directory")
        .join(format!(".{}", name).as_str())
    }

    pub fn height<P: AsRef<Path>>(home: P) -> Result<u64> {
        let home = home.as_ref();

        if !home.exists() {
            return Ok(0);
        }

        let store = MerkStore::new(home.join("merk"));
        store.height()
    }
}

#[derive(Default)]
pub struct DefaultConfig {
    pub seeds: Option<String>,
    pub timeout_commit: Option<String>,
}

impl<A: App> Node<A> {
    pub fn new<P: AsRef<Path>>(
        home: P,
        chain_id: Option<&str>,
        cfg_defaults: DefaultConfig,
    ) -> Self {
        let home = home.as_ref().to_path_buf();
        let merk_home = home.join("merk");
        let tm_home = home.join("tendermint");

        if !home.exists() {
            std::fs::create_dir_all(&home)
                .expect("Failed to initialize application home directory");
        }

        let cfg_path = tm_home.join("config/config.toml");
        let tm_previously_configured = cfg_path.exists();
        let _ = Tendermint::new(tm_home.clone())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .init();

        let read_toml = || {
            let config =
                std::fs::read_to_string(&cfg_path).expect("Failed to read Tendermint config");
            config
                .parse::<toml_edit::Document>()
                .expect("Failed to parse toml")
        };

        let write_toml = |toml: toml_edit::Document| {
            std::fs::write(&cfg_path, toml.to_string()).expect("Failed to write Tendermint config");
        };

        if !tm_previously_configured {
            if let Some(seeds) = cfg_defaults.seeds {
                let mut toml = read_toml();
                toml["p2p"]["seeds"] = toml_edit::value(seeds);
                write_toml(toml);
            }

            if let Some(timeout_commit) = cfg_defaults.timeout_commit {
                let mut toml = read_toml();
                toml["consensus"]["timeout_commit"] = toml_edit::value(timeout_commit);
                write_toml(toml);
            }

            let mut genesis_json: serde_json::Value =
                std::fs::read_to_string(tm_home.join("config/genesis.json"))
                    .expect("Failed to read genesis.json")
                    .parse()
                    .unwrap();
            if let Some(chain_id) = chain_id {
                genesis_json["chain_id"] = serde_json::Value::String(chain_id.to_string());
            }
            std::fs::write(
                tm_home.join("config/genesis.json"),
                serde_json::to_string_pretty(&genesis_json).unwrap(),
            )
            .expect("Failed to modify genesis chain ID");
        }

        let abci_port: u16 = if cfg_path.exists() {
            let toml = read_toml();
            let abci_laddr = toml["proxy_app"]
                .as_str()
                .expect("config.toml is missing proxy_app");

            abci_laddr
                .rsplit(':')
                .next()
                .expect("Failed to parse abci_laddr")
                .parse()
                .expect("Failed to parse proxy_app port")
        } else {
            26658
        };

        Node {
            _app: PhantomData,
            merk_home,
            tm_home,
            home,
            abci_port,
            genesis_bytes: None,
            p2p_persistent_peers: None,
            skip_init_chain: false,
            stdout: Stdio::null(),
            stderr: Stdio::null(),
            logs: false,
            flags: vec![],
        }
    }

    pub fn run(self) -> Result<()> {
        // Start tendermint process
        let tm_home = self.tm_home.clone();
        let abci_port = self.abci_port;
        let stdout = self.stdout;
        let stderr = self.stderr;
        let maybe_genesis_bytes = self.genesis_bytes;
        let maybe_peers = self.p2p_persistent_peers;

        let mut tm_process = Tendermint::new(&tm_home)
            .stdout(stdout)
            .stderr(stderr)
            .logs(self.logs)
            .flags(self.flags)
            .proxy_app(format!("tcp://0.0.0.0:{}", abci_port).as_str());

        if let Some(genesis_bytes) = maybe_genesis_bytes {
            tm_process = tm_process.with_genesis(genesis_bytes);
        }

        if let Some(peers) = maybe_peers {
            tm_process = tm_process.p2p_persistent_peers(peers);
        }

        tm_process = tm_process.start();

        let genesis: serde_json::Value =
            std::fs::read_to_string(self.tm_home.join("config/genesis.json"))?
                .parse()
                .unwrap();
        let chain_id = genesis["chain_id"].as_str().unwrap();
        Context::add(crate::plugins::ChainId(chain_id.to_string()));

        let app = InternalApp::<ABCIPlugin<A>>::new();
        let store = MerkStore::new(self.merk_home.clone());

        let res = ABCIStateMachine::new(app, store, self.skip_init_chain)
            .listen(format!("127.0.0.1:{}", self.abci_port));
        tm_process.kill()?;

        if let Err(crate::Error::Upgrade(crate::upgrade::Error::Version { expected, actual })) = res
        {
            log::warn!(
                "Node is version {}, but network is version {}",
                hex::encode(actual.to_vec()),
                hex::encode(expected.to_vec()),
            );

            std::fs::write(
                self.home.join("network_version"),
                format!("{}\n", hex::encode(expected.to_vec())),
            )?;

            std::process::exit(138);
        }

        res
    }

    #[must_use]
    pub fn reset(self) -> Self {
        if self.merk_home.exists() {
            std::fs::remove_dir_all(&self.merk_home).expect("Failed to clear Merk data");
        }

        Tendermint::new(&self.tm_home)
            .stdout(std::process::Stdio::null())
            .unsafe_reset_all();

        self
    }

    // TODO: remove when we don't require compat migrations
    pub fn migrate<O: State>(self, version: Vec<u8>) -> Self
    where
        ABCIPlugin<O>: Migrate,
    {
        let merk_store = crate::merk::MerkStore::new(&self.merk_home);
        if merk_store
            .merk()
            .get_aux(b"consensus_version")
            .unwrap()
            .is_some()
        {
            return self;
        }

        log::info!("Migrating store data... (This might take a while)");
        let store = Shared::new(merk_store);
        let mut store = Store::new(BackingStore::Merk(store));
        let bytes = store.get(&[]).unwrap().unwrap();

        orga::set_compat_mode(true);
        let mut app =
            ABCIPlugin::<O>::migrate(store.clone(), store.clone(), &mut bytes.as_slice()).unwrap();
        orga::set_compat_mode(false);

        app.attach(store.clone()).unwrap();
        let mut bytes = vec![];
        app.flush(&mut bytes).unwrap();
        store.put(vec![], bytes).unwrap();
        if let BackingStore::Merk(merk_store) = store.into_backing_store().into_inner() {
            merk_store
                .into_inner()
                .write(vec![(b"consensus_version".to_vec(), Some(version))])
                .unwrap();
        } else {
            unreachable!();
        }

        self
    }

    pub fn skip_init_chain(mut self) -> Self {
        self.skip_init_chain = true;

        self
    }

    pub fn init_from_store(self, source: impl AsRef<Path>, height: Option<u64>) -> Self {
        MerkStore::init_from(source, &self.merk_home, height).unwrap();

        self
    }

    #[must_use]
    pub fn with_genesis<const N: usize>(mut self, genesis_bytes: &'static [u8; N]) -> Self {
        self.genesis_bytes.replace(genesis_bytes.to_vec());

        self
    }

    #[must_use]
    pub fn peers<T: Borrow<str>>(mut self, peers: &[T]) -> Self {
        let peers = peers.iter().map(|p| p.borrow().to_string()).collect();
        self.p2p_persistent_peers.replace(peers);

        self
    }

    #[must_use]
    pub fn stdout<T: Into<Stdio>>(mut self, stdout: T) -> Self {
        self.stdout = stdout.into();

        self
    }

    #[must_use]
    pub fn stderr<T: Into<Stdio>>(mut self, stderr: T) -> Self {
        self.stderr = stderr.into();

        self
    }

    #[must_use]
    pub fn print_tendermint_logs(mut self, logs: bool) -> Self {
        self.logs = logs;

        self
    }

    #[must_use]
    pub fn tendermint_flags(mut self, flags: Vec<String>) -> Self {
        self.flags = flags;

        self
    }
}

impl<A: App> InternalApp<ABCIPlugin<A>> {
    fn run<T, F: FnOnce(&mut ABCIPlugin<A>) -> T>(&self, store: WrappedMerk, op: F) -> Result<T> {
        let mut store = Store::new(store.into());
        let state_bytes = match store.get(&[])? {
            Some(inner) => inner,
            None => {
                let mut default: ABCIPlugin<A> = Default::default();
                // TODO: should the real store actually be passed in here?
                default.attach(store.clone())?;
                let mut encoded_bytes = vec![];
                default.flush(&mut encoded_bytes)?;

                store.put(vec![], encoded_bytes.clone())?;
                encoded_bytes
            }
        };
        let mut state: ABCIPlugin<A> =
            ABCIPlugin::<A>::load(store.clone(), &mut state_bytes.as_slice())?;
        let res = op(&mut state);
        let mut bytes = vec![];
        state.flush(&mut bytes)?;
        store.put(vec![], bytes)?;
        Ok(res)
    }
}

impl<A: App> Application for InternalApp<ABCIPlugin<A>> {
    fn init_chain(&self, store: WrappedMerk, req: RequestInitChain) -> Result<ResponseInitChain> {
        let mut updates = self.run(store, move |state| -> Result<_> {
            state.call(req.into())?;
            Ok(state
                .validator_updates
                .take()
                .expect("ABCI plugin did not create initial validator updates"))
        })??;
        let mut res: ResponseInitChain = Default::default();
        updates.drain().for_each(|(_key, update)| {
            res.validators.push(update);
        });

        Ok(res)
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
                .expect("ABCI plugin did not create validator update map"))
        })??;

        // Write back validator updates
        let mut res: ResponseEndBlock = Default::default();
        updates.drain().for_each(|(_key, update)| {
            if let Ok(flag) = std::env::var("ORGA_STATIC_VALSET") {
                if flag != "0" && flag != "false" {
                    return;
                }
            }
            res.validator_updates.push(update);
        });

        Ok(res)
    }

    fn deliver_tx(&self, store: WrappedMerk, req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        let run_res = self.run(store, move |state| -> Result<_> {
            let inner_call = Decode::decode(req.tx.to_vec().as_slice())?;
            let res = state.call(ABCICall::DeliverTx(inner_call));

            Ok((
                res,
                state.events.take().unwrap_or_default(),
                state.logs.take().unwrap_or_default(),
            ))
        })?;

        let mut deliver_tx_res = ResponseDeliverTx::default();
        match run_res {
            Ok((res, events, logs)) => match res {
                Ok(()) => {
                    deliver_tx_res.code = 0;
                    deliver_tx_res.log = logs.join("\n");
                    deliver_tx_res.events = events;
                }
                Err(err) => {
                    deliver_tx_res.code = 1;
                    if logs.is_empty() {
                        deliver_tx_res.log = err.to_string();
                    } else {
                        deliver_tx_res.log = logs.join("\n");
                    }
                }
            },
            Err(err) => {
                deliver_tx_res.code = 1;
                deliver_tx_res.log = err.to_string();
            }
        }

        Ok(deliver_tx_res)
    }

    fn check_tx(&self, store: WrappedMerk, req: RequestCheckTx) -> Result<ResponseCheckTx> {
        let run_res = self.run(store, move |state| -> Result<_> {
            let inner_call = Decode::decode(req.tx.to_vec().as_slice())?;
            let res = state.call(ABCICall::CheckTx(inner_call));

            Ok((
                res,
                state.events.take().unwrap_or_default(),
                state.logs.take().unwrap_or_default(),
            ))
        })?;

        let mut check_tx_res = ResponseCheckTx::default();

        match run_res {
            Ok((res, events, logs)) => match res {
                Ok(()) => {
                    check_tx_res.code = 0;
                    check_tx_res.log = logs.join("\n");
                    check_tx_res.events = events;
                }
                Err(err) => {
                    check_tx_res.code = 1;
                    if logs.is_empty() {
                        check_tx_res.log = err.to_string();
                    } else {
                        check_tx_res.log = logs.join("\n");
                    }
                }
            },
            Err(err) => {
                check_tx_res.code = 1;
                check_tx_res.log = err.to_string();
            }
        }

        Ok(check_tx_res)
    }

    fn query(&self, merk_store: Shared<MerkStore>, req: RequestQuery) -> Result<ResponseQuery> {
        let create_state = |store| {
            let store = Store::new(store);
            let state_bytes = store
                .get(&[])?
                .ok_or_else(|| crate::Error::Query("Store is empty".to_string()))?;
            ABCIPlugin::<A>::load(store, &mut state_bytes.as_slice())
        };
        let store = merk_store.borrow();
        let (height, snapshot) = if req.height == 0 {
            store.snapshots().get_latest()
        } else {
            store
                .snapshots()
                .get(req.height.try_into()?)
                .map(|s| (req.height as u64, s))
        }
        .ok_or_else(|| crate::Error::Query(format!("Cannot query for height {}", req.height)))?;

        if !req.path.is_empty() {
            let store = BackingStore::Snapshot(Shared::new(snapshot.clone()));
            let state = create_state(store)?;
            let mut res = state.abci_query(&req)?;
            res.height = height.try_into().unwrap();
            return Ok(res);
        }

        let query = Decode::decode(&*req.data)?;
        let store =
            BackingStore::ProofBuilderSnapshot(ProofBuilder::new(Shared::new(snapshot.clone())));
        let state = create_state(store.clone())?;
        state.query(query)?;

        let root_hash = snapshot.checkpoint.as_ref().borrow().root_hash();
        let proof_builder = store.into_proof_builder_snapshot()?;
        let proof_bytes = proof_builder.build()?;

        // TODO: we shouldn't need to include the root hash in the response
        let mut value = vec![];
        value.extend(root_hash);
        value.extend(proof_bytes);

        let res = ResponseQuery {
            code: 0,
            height: height.try_into()?,
            value: value.into(),
            ..Default::default()
        };
        Ok(res)
    }
}

struct InternalApp<A> {
    _app: PhantomData<A>,
}

impl<A: App> InternalApp<ABCIPlugin<A>> {
    pub fn new() -> Self {
        Self { _app: PhantomData }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        abci::{BeginBlock, Node},
        client::{wallet::Unsigned, AppClient},
        coins::Symbol,
        context::Context,
        plugins::{ChainId, ConvertSdkTx, DefaultPlugins, PaidCall},
        tendermint::client::HttpClient,
    };

    use super::*;
    use orga::orga;

    #[orga]
    #[derive(Debug, Clone, Copy)]
    pub struct FooCoin();

    impl Symbol for FooCoin {
        const INDEX: u8 = 123;
        const NAME: &'static str = "FOO";
    }

    #[orga]
    pub struct App {
        pub count: u32,
    }

    impl BeginBlock for App {
        fn begin_block(&mut self, _ctx: &orga::plugins::BeginBlockCtx) -> Result<()> {
            self.count += 1;

            Ok(())
        }
    }

    // TODO: dedupe w/ tendermint::client tests
    pub fn spawn_node() {
        pretty_env_logger::init();

        std::thread::spawn(move || {
            // TODO: find available ports

            Context::add(ChainId("foo".to_string()));

            let home = tempdir::TempDir::new("orga-node").unwrap();
            let node = Node::<DefaultPlugins<FooCoin, App>>::new(
                home.path(),
                Some("foo"),
                orga::abci::DefaultConfig {
                    seeds: None,
                    timeout_commit: None,
                },
            );
            node.run().unwrap();
            home.close().unwrap();
        });

        // TODO: wait for node to be ready

        // TODO: return type which kills node after drop
        // TODO: return client which talks to the node (or just RPC address)
    }

    impl ConvertSdkTx for App {
        type Output = PaidCall<<App as Call>::Call>;

        fn convert(
            &self,
            _msg: &crate::plugins::sdk_compat::sdk::Tx,
        ) -> orga::Result<Self::Output> {
            todo!()
        }
    }

    #[ignore]
    #[tokio::test]
    #[serial_test::serial]
    async fn historical_queries() -> Result<()> {
        spawn_node();

        // TODO: node spawn should wait for node to be ready
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        for i in 1..5 {
            let client = HttpClient::with_height("http://localhost:26657", i).unwrap();
            let client = AppClient::<App, App, _, FooCoin, _>::new(client, Unsigned);
            assert_eq!(client.query(|app| Ok(app.count)).await.unwrap(), i);
        }

        Ok(())
    }
}
