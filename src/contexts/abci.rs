// use super::Context;
use crate::abci::App;
use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use tendermint_proto::abci::ValidatorUpdate;
use tendermint_proto::crypto::{public_key::Sum, PublicKey};

pub struct ABCIProvider<T> {
    inner: T,
    pub(crate) validator_updates: Option<HashMap<[u8; 32], ValidatorUpdate>>,
}

pub struct InitChainCtx {}
pub struct BeginBlockCtx {
    pub height: u64,
}

#[derive(Default)]
pub struct EndBlockCtx {
    pub validator_updates: RefCell<HashMap<[u8; 32], ValidatorUpdate>>,
}

impl EndBlockCtx {
    pub fn set_voting_power(&self, pub_key: [u8; 32], power: u64) {
        let sum = Some(Sum::Ed25519(pub_key.to_vec()));
        let key = PublicKey { sum };
        let mut validator_updates = self.validator_updates.borrow_mut();
        validator_updates.insert(
            pub_key,
            tendermint_proto::abci::ValidatorUpdate {
                pub_key: Some(key),
                power: power as i64,
            },
        );
    }
}

#[derive(Encode, Decode)]
pub enum ABCICall<C> {
    InitChain,
    BeginBlock { height: u64 },
    EndBlock,
    DeliverTx(C),
    CheckTx(C),
}

impl<T: App> Call for ABCIProvider<T> {
    type Call = ABCICall<T::Call>;
    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.reset();
        use ABCICall::*;
        match call {
            InitChain => {
                let ctx = InitChainCtx {};
                self.inner.init_chain(&ctx)?;

                Ok(())
            }
            BeginBlock { height } => {
                let ctx = BeginBlockCtx { height };
                self.inner.begin_block(&ctx)?;

                Ok(())
            }
            EndBlock => {
                let ctx = Default::default();
                self.inner.end_block(&ctx)?;
                self.validator_updates
                    .replace(ctx.validator_updates.into_inner());

                Ok(())
            }
            DeliverTx(inner_call) => self.inner.call(inner_call),
            CheckTx(inner_call) => self.inner.call(inner_call),
        }
    }
}

impl<T: App> ABCIProvider<T> {
    pub fn reset(&mut self) {
        self.validator_updates = None;
    }
}

impl<T: Query> Query for ABCIProvider<T> {
    type Query = T::Query;
    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<T> State for ABCIProvider<T>
where
    T: State,
    T::Encoding: From<T>,
{
    type Encoding = (T::Encoding,);
    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store, data.0)?,
            validator_updates: None,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<T> From<ABCIProvider<T>> for (T::Encoding,)
where
    T: State,
    T::Encoding: From<T>,
{
    fn from(provider: ABCIProvider<T>) -> Self {
        (provider.inner.into(),)
    }
}
