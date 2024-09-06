use crate::abci::{prost::Adapter, AbciQuery, App};
use crate::call::Call;
use crate::collections::{Entry, EntryMap, Map};
use crate::context::Context;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::Migrate;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{compat_mode, Error, Result};
use serde::Serialize;
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::convert::TryInto;
use std::rc::Rc;
use tendermint_proto::google::protobuf::Timestamp;
use tendermint_proto::v0_34::abci::Event;
use tendermint_proto::v0_34::abci::{Evidence, LastCommitInfo, RequestQuery, ResponseQuery};
use tendermint_proto::v0_34::abci::{
    RequestBeginBlock, RequestEndBlock, RequestInitChain, ValidatorUpdate,
};
use tendermint_proto::v0_34::crypto::{public_key::Sum, PublicKey};
use tendermint_proto::v0_34::types::Header;

/// The (BFT) timestamp of the block for which calls are currently being
/// executed.
pub struct Time {
    /// Unix seconds
    pub seconds: i64,
    /// Non-negative fractions of a second at nanosecond resolution, directly
    /// from ABCI block timestamp via `tendermint_proto` (which also uses an
    /// i32).
    pub nanos: i32,
}

impl Time {
    // #[cfg(test)]
    /// Create a time context from unix seconds, assuming 0 nanoseconds.
    pub fn from_seconds<T: Into<i64>>(seconds: T) -> Self {
        let seconds = seconds.into();
        Self { seconds, nanos: 0 }
    }
}

impl<T: Into<i64>> From<T> for Time {
    fn from(seconds: T) -> Self {
        Self::from_seconds(seconds)
    }
}

/// A validator entry, whose key is the validator's consensus public key.
#[derive(Entry, Clone)]
pub struct ValidatorEntry {
    /// Validator consensus public key.
    #[key]
    pub pubkey: [u8; 32],
    /// Voting power.
    pub power: u64,
}

type UpdateMap = Map<[u8; 32], Adapter<ValidatorUpdate>>;

/// A plugin for interfacing with ABCI.
///
/// State-changing ABCI messages are handled by this plugin's [Call]
/// implementation.
///
/// Interfaces for reading or changing the validator set, emitting logs or
/// events, etc. are provided via [Context].
#[derive(Serialize)]
pub struct ABCIPlugin<T> {
    /// The inner value.
    pub inner: T,
    #[serde(skip)]
    pub(crate) validator_updates: Option<HashMap<[u8; 32], ValidatorUpdate>>,
    #[serde(skip)]
    updates: UpdateMap,
    #[serde(skip)]
    time: Option<Timestamp>,
    #[serde(skip)]
    pub(crate) events: Option<Vec<Event>>,
    #[serde(skip)]
    current_vp: Rc<RefCell<Option<EntryMap<ValidatorEntry>>>>,
    #[serde(skip)]
    cons_key_by_op_addr: Rc<RefCell<Option<OperatorMap>>>,
    #[serde(skip)]
    pub(crate) logs: Option<Vec<String>>,
}

impl<T: Migrate> Migrate for ABCIPlugin<T> {
    fn migrate(src: Store, dest: Store, bytes: &mut &[u8]) -> Result<Self> {
        if !compat_mode() {
            *bytes = &bytes[1..];
        }
        Ok(Self {
            inner: T::migrate(src.sub(&[0]), dest.sub(&[0]), bytes)?,
            validator_updates: None,
            updates: State::load(src.sub(&[1]), bytes)?,
            current_vp: Rc::new(RefCell::new(Some(State::load(src.sub(&[2]), bytes)?))),
            cons_key_by_op_addr: Rc::new(RefCell::new(Some(State::load(src.sub(&[3]), bytes)?))),
            events: None,
            time: None,
            logs: None,
        })
    }
}

impl<T: Default> Default for ABCIPlugin<T> {
    fn default() -> Self {
        Self {
            inner: T::default(),
            validator_updates: None,
            updates: UpdateMap::default(),
            time: None,
            events: None,
            current_vp: Rc::new(RefCell::new(Some(Default::default()))),
            cons_key_by_op_addr: Rc::new(RefCell::new(Some(Default::default()))),
            logs: None,
        }
    }
}

/// Context for the `InitChain` ABCI message.
pub struct InitChainCtx {
    /// Timestamp from the chain's genesis.
    pub time: Option<Timestamp>,
    /// Chain ID.
    pub chain_id: String,
    /// Initial validator set.
    pub validators: Vec<Validator>,
    /// Initial app state bytes.
    pub app_state_bytes: Vec<u8>,
    /// Starting height of the chain.
    pub initial_height: i64,
}

/// A validator in the validator set.
#[derive(Encode, Decode, Debug)]
pub struct Validator {
    /// Consensus public key.
    pub pubkey: [u8; 32],
    /// Voting power.
    pub power: u64,
}

impl From<ValidatorUpdate> for Validator {
    fn from(update: ValidatorUpdate) -> Self {
        let pubkey_bytes = match update.pub_key.unwrap().sum.unwrap() {
            Sum::Ed25519(bytes) => bytes,
            Sum::Secp256k1(bytes) => bytes,
        };

        let pubkey: [u8; 32] = pubkey_bytes.try_into().unwrap();
        let power: u64 = update.power.try_into().unwrap();

        Validator { pubkey, power }
    }
}

impl From<RequestInitChain> for InitChainCtx {
    fn from(req: RequestInitChain) -> Self {
        let validators = req.validators.into_iter().map(Into::into).collect();

        Self {
            time: req.time,
            chain_id: req.chain_id,
            validators,
            app_state_bytes: req.app_state_bytes.to_vec(),
            initial_height: req.initial_height,
        }
    }
}

/// Context for the `BeginBlock` ABCI message.
pub struct BeginBlockCtx {
    /// The block's hash.
    pub hash: Vec<u8>,
    /// The block's height.
    pub height: u64,
    /// Block header
    pub header: Header,
    /// Last commit info.
    pub last_commit_info: Option<LastCommitInfo>,
    /// Evidence of bad behavior by validators.
    pub byzantine_validators: Vec<Evidence>,
}

impl From<RequestBeginBlock> for BeginBlockCtx {
    fn from(req: RequestBeginBlock) -> Self {
        let header = req.header.expect("Missing header in BeginBlock");
        let height = header.height as u64;

        BeginBlockCtx {
            header,
            height,
            hash: req.hash.to_vec(),
            last_commit_info: req.last_commit_info,
            byzantine_validators: req.byzantine_validators,
        }
    }
}

/// Context for the `EndBlock` ABCI message.
#[cfg_attr(test, derive(Default))]
pub struct EndBlockCtx {
    /// The block's height.
    pub height: u64,
}

impl From<RequestEndBlock> for EndBlockCtx {
    fn from(req: RequestEndBlock) -> Self {
        EndBlockCtx {
            height: req.height as u64,
        }
    }
}

type OperatorMap = Map<[u8; 20], [u8; 32]>;

/// A context for reading or updating the validator set.
pub struct Validators {
    pub(crate) updates: HashMap<[u8; 32], Adapter<ValidatorUpdate>>,
    current_vp: Rc<RefCell<Option<EntryMap<ValidatorEntry>>>>,
    cons_key_by_op_addr: Rc<RefCell<Option<OperatorMap>>>,
}

impl Validators {
    /// Creates a new `Validators` context instance.
    pub fn new(
        current_vp: Rc<RefCell<Option<EntryMap<ValidatorEntry>>>>,
        cons_key_by_op_addr: Rc<RefCell<Option<OperatorMap>>>,
    ) -> Self {
        Self {
            updates: HashMap::new(),
            current_vp,
            cons_key_by_op_addr,
        }
    }

    /// Set the voting power of a validator by consensus key.
    pub fn set_voting_power<A: Into<[u8; 32]>>(&mut self, pub_key: A, power: u64) {
        let pub_key = pub_key.into();

        let sum = Some(Sum::Ed25519(pub_key.to_vec()));
        let key = PublicKey { sum };
        self.current_vp
            .borrow_mut()
            .as_mut()
            .unwrap() // TODO: return a result instead
            .insert(ValidatorEntry {
                power,
                pubkey: pub_key,
            })
            .unwrap();
        self.updates.insert(
            pub_key,
            Adapter(tendermint_proto::v0_34::abci::ValidatorUpdate {
                pub_key: Some(key),
                power: power as i64,
            }),
        );
    }

    /// Sets the operator address for a validator by consensus key.
    pub fn set_operator<A: Into<[u8; 32]>, B: Into<[u8; 20]>>(
        &mut self,
        consensus_key: A,
        operator_address: B,
    ) -> Result<()> {
        let pub_key = consensus_key.into();
        let op_addr = operator_address.into();
        self.cons_key_by_op_addr
            .borrow_mut()
            .as_mut()
            .unwrap()
            .insert(op_addr, pub_key)
    }

    /// Returns the consensus key for an operator address.
    pub fn consensus_key<A: Into<[u8; 20]>>(&self, op_key: A) -> Result<Option<[u8; 32]>> {
        let op_addr = op_key.into();
        Ok(self
            .cons_key_by_op_addr
            .borrow()
            .as_ref()
            .unwrap()
            .get(op_addr)?
            .map(|v| *v))
    }

    /// Returns the current validator set.
    pub fn current_set(&mut self) -> Ref<Option<EntryMap<ValidatorEntry>>> {
        self.current_vp.borrow()
    }

    /// Returns the total voting power of the validator set.
    pub fn total_voting_power(&mut self) -> Result<u64> {
        let mut sum = 0;
        for entry in self
            .current_vp
            .borrow()
            .as_ref()
            .ok_or_else(|| Error::App("Validator set not available".to_string()))?
            .iter()?
        {
            let v = entry?;
            sum += v.power;
        }

        Ok(sum)
    }

    /// Returns a list of all validators in the validator set.
    pub fn entries(&mut self) -> Result<Vec<ValidatorEntry>> {
        let mut res = vec![];
        for entry in self
            .current_vp
            .borrow()
            .as_ref()
            .ok_or_else(|| Error::App("Validator set not available".to_string()))?
            .iter()?
        {
            let v = entry?;
            res.push(v.clone());
        }

        Ok(res)
    }
}

/// A context for emitting events via ABCI responses.
#[derive(Default)]
pub struct Events {
    pub(crate) events: Vec<Event>,
}

impl Events {
    /// Emit an event.
    pub fn add(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Read the events that have been emitted during the current ABCI call.
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

/// A context for emitting log messages via ABCI responses.
#[derive(Default)]
pub struct Logs {
    pub(crate) messages: Vec<String>,
}

impl Logs {
    /// Emit a log message.
    pub fn add(&mut self, message: impl AsRef<str>) {
        self.messages.push(message.as_ref().to_string());
    }
}

/// Call variants for ABCI message types.
#[derive(Debug, Encode, Decode)]
pub enum ABCICall<C> {
    /// The `InitChain` ABCI message.
    InitChain(Adapter<RequestInitChain>),
    /// The `BeginBlock` ABCI message.
    BeginBlock(Box<Adapter<RequestBeginBlock>>), /* Boxed because this variant is much larger
                                                  * than the others */
    /// The `EndBlock` ABCI message.
    EndBlock(Adapter<RequestEndBlock>),
    /// The `DeliverTx` ABCI message.
    DeliverTx(C),
    /// The `CheckTx` ABCI message.
    CheckTx(C),
}

impl<C> From<RequestInitChain> for ABCICall<C> {
    fn from(req: RequestInitChain) -> Self {
        ABCICall::InitChain(Adapter(req))
    }
}

impl<C> From<RequestBeginBlock> for ABCICall<C> {
    fn from(req: RequestBeginBlock) -> Self {
        ABCICall::BeginBlock(Box::new(Adapter(req)))
    }
}

impl<C> From<RequestEndBlock> for ABCICall<C> {
    fn from(req: RequestEndBlock) -> Self {
        ABCICall::EndBlock(Adapter(req))
    }
}

impl<T: App> Call for ABCIPlugin<T> {
    type Call = ABCICall<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        use ABCICall::*;
        let validators = Validators::new(self.current_vp.clone(), self.cons_key_by_op_addr.clone());
        let context_remover = ContextRemover;
        Context::add(validators);
        let create_time_ctx = |time: &Option<Timestamp>| {
            if let Some(timestamp) = time {
                Context::add(Time {
                    seconds: timestamp.seconds,
                    nanos: timestamp.nanos,
                });
            }
        };
        create_time_ctx(&self.time);

        match call {
            InitChain(req) => {
                let ctx: InitChainCtx = req.into_inner().into();
                self.time = ctx.time;
                create_time_ctx(&self.time);
                self.inner.init_chain(&ctx)?;
            }
            BeginBlock(req) => {
                Context::add(Events::default());
                Context::add(Logs::default());
                self.events.replace(vec![]);
                self.logs.replace(vec![]);
                let ctx: BeginBlockCtx = req.into_inner().into();
                self.time = ctx.header.clone().time;
                create_time_ctx(&self.time);
                let res = self.inner.begin_block(&ctx);
                if res.is_ok() {
                    self.events
                        .replace(Context::resolve::<Events>().unwrap().events.clone());
                }
                self.logs
                    .replace(Context::resolve::<Logs>().unwrap().messages.clone());
                Context::remove::<Events>();
                Context::remove::<Logs>();

                res?;
            }
            EndBlock(req) => {
                Context::add(Events::default());
                Context::add(Logs::default());
                self.events.replace(vec![]);
                self.logs.replace(vec![]);
                let ctx = req.into_inner().into();
                let res = self.inner.end_block(&ctx);
                if res.is_ok() {
                    self.events
                        .replace(Context::resolve::<Events>().unwrap().events.clone());
                }
                self.logs
                    .replace(Context::resolve::<Logs>().unwrap().messages.clone());
                Context::remove::<Events>();
                Context::remove::<Logs>();
                res?;
            }
            DeliverTx(inner_call) => {
                Context::add(Events::default());
                Context::add(Logs::default());
                self.events.replace(vec![]);
                self.logs.replace(vec![]);
                let res = self.inner.call(inner_call);
                if res.is_ok() {
                    self.events
                        .replace(Context::resolve::<Events>().unwrap().events.clone());
                }
                self.logs
                    .replace(Context::resolve::<Logs>().unwrap().messages.clone());
                Context::remove::<Events>();
                Context::remove::<Logs>();
                res?;
            }
            CheckTx(inner_call) => {
                Context::add(Events::default());
                Context::add(Logs::default());
                self.events.replace(vec![]);
                self.logs.replace(vec![]);
                let res = self.inner.call(inner_call);
                if res.is_ok() {
                    self.events
                        .replace(Context::resolve::<Events>().unwrap().events.clone());
                }
                self.logs
                    .replace(Context::resolve::<Logs>().unwrap().messages.clone());
                Context::remove::<Events>();
                Context::remove::<Logs>();
                res?;
            }
        };

        let validators = Context::resolve::<Validators>().unwrap();
        let mut current_vp_ref = validators.current_vp.borrow_mut();
        let current_vp = current_vp_ref.as_mut().unwrap();
        for (pubkey, update) in validators.updates.iter() {
            self.updates.insert(*pubkey, Adapter((*update).clone()))?;
            if update.power > 0 {
                current_vp.insert(ValidatorEntry {
                    pubkey: *pubkey,
                    power: update.power as u64,
                })?;
            } else {
                current_vp.delete(ValidatorEntry {
                    pubkey: *pubkey,
                    power: 0,
                })?;
            }
        }

        self.build_updates()?;
        context_remover.remove();
        Ok(())
    }
}

struct ContextRemover;

impl ContextRemover {
    fn remove(&self) {
        Context::remove::<Validators>();
    }
}

impl Drop for ContextRemover {
    fn drop(&mut self) {
        self.remove();
    }
}

impl<T: App> ABCIPlugin<T> {
    fn build_updates(&mut self) -> Result<()> {
        let mut update_keys = vec![];
        let mut update_map = HashMap::new();

        for entry in self.updates.iter()? {
            let (pubkey, update) = entry?;
            update_map.insert(*pubkey, update.clone());
            update_keys.push(*pubkey);
        }

        // Clear the update map
        for key in update_keys {
            self.updates.remove(key)?;
        }

        // Expose validator updates for use in node
        self.validator_updates.replace(update_map);
        Ok(())
    }
}

impl<T: Query> Query for ABCIPlugin<T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<T: State> State for ABCIPlugin<T> {
    fn attach(&mut self, store: Store) -> Result<()> {
        self.inner.attach(store.sub(&[0]))?;
        self.updates.attach(store.sub(&[1]))?;
        self.current_vp.borrow_mut().attach(store.sub(&[2]))?;
        self.cons_key_by_op_addr
            .borrow_mut()
            .attach(store.sub(&[3]))
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        if !compat_mode() {
            out.write_all(&[0])?;
        }
        self.inner.flush(out)?;
        self.updates.flush(out)?;
        self.current_vp.take().flush(&mut vec![])?;
        self.cons_key_by_op_addr.take().flush(&mut vec![])
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        let mut loader = crate::state::Loader::new(store, bytes, 0);

        Ok(Self {
            inner: loader.load_child::<Self, _>()?,
            validator_updates: None,
            updates: loader.load_child::<Self, _>()?,
            current_vp: Rc::new(RefCell::new(Some(loader.load_child::<Self, _>()?))),
            cons_key_by_op_addr: Rc::new(RefCell::new(Some(loader.load_child::<Self, _>()?))),
            events: None,
            time: None,
            logs: None,
        })
    }

    fn field_keyop(field_name: &str) -> Option<crate::describe::KeyOp> {
        match field_name {
            "inner" => Some(crate::describe::KeyOp::Append(vec![0])),
            "updates" => Some(crate::describe::KeyOp::Append(vec![1])),
            "current_vp" => Some(crate::describe::KeyOp::Append(vec![2])),
            "cons_key_by_op_addr" => Some(crate::describe::KeyOp::Append(vec![3])),
            _ => None,
        }
    }
}

impl<T: State + Describe> Describe for ABCIPlugin<T> {
    fn describe() -> crate::describe::Descriptor {
        crate::describe::Builder::new::<Self>()
            .meta::<T>()
            .named_child::<T>("inner", &[0])
            // TODO: other fields
            .build()
    }
}

impl<T> AbciQuery for ABCIPlugin<T>
where
    T: State + AbciQuery,
{
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        self.inner.abci_query(req)
    }
}
