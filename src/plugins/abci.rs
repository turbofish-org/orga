pub struct Time {
    pub seconds: i64,
    pub nanos: i32,
}

impl Time {
    #[cfg(test)]
    pub(crate) fn from_seconds<T: Into<i64>>(seconds: T) -> Self {
        let seconds = seconds.into();
        Self { seconds, nanos: 0 }
    }
}

#[cfg(feature = "abci")]
mod full {
    use super::Time;
    use crate::abci::{prost::Adapter, App};
    use crate::call::Call;
    use crate::collections::{Entry, EntryMap, Map};
    use crate::context::Context;
    use crate::encoding::{Decode, Encode};
    use crate::query::Query;
    use crate::state::State;
    use crate::store::Store;
    use crate::Result;
    #[cfg(test)]
    use mutagen::mutate;
    use std::cell::{Ref, RefCell};
    use std::collections::HashMap;
    use std::convert::TryInto;
    use std::rc::Rc;
    use tendermint_proto::abci::{
        Evidence, LastCommitInfo, RequestBeginBlock, RequestEndBlock, RequestInitChain,
        ValidatorUpdate,
    };
    use tendermint_proto::crypto::{public_key::Sum, PublicKey};
    use tendermint_proto::google::protobuf::Timestamp;
    use tendermint_proto::types::Header;

    #[derive(Entry)]
    pub struct ValidatorEntry {
        #[key]
        pub pubkey: [u8; 32],
        pub power: u64,
    }

    type UpdateMap = Map<[u8; 32], Adapter<ValidatorUpdate>>;
    pub struct ABCIPlugin<T> {
        inner: T,
        pub(crate) validator_updates: Option<HashMap<[u8; 32], ValidatorUpdate>>,
        updates: UpdateMap,
        time: Option<Timestamp>,
        current_vp: Rc<RefCell<Option<EntryMap<ValidatorEntry>>>>,
    }

    pub struct InitChainCtx {
        pub time: Option<Timestamp>,
        pub chain_id: String,
        pub validators: Vec<Validator>,
        pub app_state_bytes: Vec<u8>,
        pub initial_height: i64,
    }

    pub struct Validator {
        pub pubkey: [u8; 32],
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
                app_state_bytes: req.app_state_bytes,
                initial_height: req.initial_height,
            }
        }
    }

    pub struct BeginBlockCtx {
        pub hash: Vec<u8>,
        pub height: u64,
        pub header: Header,
        pub last_commit_info: Option<LastCommitInfo>,
        pub byzantine_validators: Vec<Evidence>,
    }

    impl From<RequestBeginBlock> for BeginBlockCtx {
        fn from(req: RequestBeginBlock) -> Self {
            let header = req.header.expect("Missing header in BeginBlock");
            let height = header.height as u64;

            BeginBlockCtx {
                header,
                height,
                hash: req.hash,
                last_commit_info: req.last_commit_info,
                byzantine_validators: req.byzantine_validators,
            }
        }
    }

    #[derive(Default)]
    pub struct EndBlockCtx {}

    impl From<RequestEndBlock> for EndBlockCtx {
        fn from(_req: RequestEndBlock) -> Self {
            Default::default()
        }
    }

    #[derive(Default)]
    pub struct Validators {
        pub(crate) updates: HashMap<[u8; 32], Adapter<ValidatorUpdate>>,
        current_vp: Rc<RefCell<Option<EntryMap<ValidatorEntry>>>>,
    }

    impl Validators {
        fn new(current_vp: Rc<RefCell<Option<EntryMap<ValidatorEntry>>>>) -> Self {
            Self {
                updates: HashMap::new(),
                current_vp,
            }
        }

        #[cfg_attr(test, mutate)]
        pub fn set_voting_power<A: Into<[u8; 32]>>(&mut self, pub_key: A, power: u64) {
            let pub_key = pub_key.into();

            let sum = Some(Sum::Ed25519(pub_key.to_vec()));
            let key = PublicKey { sum };
            self.updates.insert(
                pub_key,
                Adapter(tendermint_proto::abci::ValidatorUpdate {
                    pub_key: Some(key),
                    power: power as i64,
                }),
            );
        }

        pub fn current_set(&mut self) -> Ref<Option<EntryMap<ValidatorEntry>>> {
            self.current_vp.borrow()
        }
    }

    #[derive(Encode, Decode)]
    pub enum ABCICall<C> {
        InitChain(Adapter<RequestInitChain>),
        BeginBlock(Box<Adapter<RequestBeginBlock>>), // Boxed because this variant is much larger than the others
        EndBlock(Adapter<RequestEndBlock>),
        DeliverTx(C),
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
            let validators = Validators::new(self.current_vp.clone());
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
            let res = match call {
                InitChain(req) => {
                    let ctx: InitChainCtx = req.into_inner().into();
                    self.time = ctx.time.clone();
                    create_time_ctx(&self.time);
                    self.inner.init_chain(&ctx)?;

                    Ok(())
                }
                BeginBlock(req) => {
                    let ctx: BeginBlockCtx = req.into_inner().into();
                    self.time = ctx.header.clone().time;
                    create_time_ctx(&self.time);
                    self.inner.begin_block(&ctx)?;

                    Ok(())
                }
                EndBlock(req) => {
                    let ctx = req.into_inner().into();
                    self.inner.end_block(&ctx)?;

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
                DeliverTx(inner_call) => self.inner.call(inner_call),
                CheckTx(inner_call) => self.inner.call(inner_call),
            }?;

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
            Ok(res)
        }
    }

    impl<T: Query> Query for ABCIPlugin<T> {
        type Query = T::Query;

        fn query(&self, query: Self::Query) -> Result<()> {
            self.inner.query(query)
        }
    }

    impl<T> State for ABCIPlugin<T>
    where
        T: State,
        T::Encoding: From<T>,
    {
        type Encoding = (T::Encoding,);
        fn create(store: Store, data: Self::Encoding) -> Result<Self> {
            Ok(Self {
                inner: T::create(store.sub(&[0]), data.0)?,
                validator_updates: None,
                updates: UpdateMap::create(store.sub(&[1]), ())?,
                time: None,
                current_vp: Rc::new(RefCell::new(Some(State::create(store.sub(&[2]), ())?))),
            })
        }

        fn flush(self) -> Result<Self::Encoding> {
            self.updates.flush()?;
            self.current_vp.borrow_mut().take().unwrap().flush()?;
            Ok((self.inner.flush()?,))
        }
    }

    impl<T> From<ABCIPlugin<T>> for (T::Encoding,)
    where
        T: State,
        T::Encoding: From<T>,
    {
        fn from(provider: ABCIPlugin<T>) -> Self {
            (provider.inner.into(),)
        }
    }
}

#[cfg(feature = "abci")]
pub use full::*;
