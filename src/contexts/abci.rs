// use super::Context;
use crate::abci::{prost::Adapter, App};
use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use tendermint_proto::abci::{
    RequestBeginBlock, RequestEndBlock, RequestInitChain, ValidatorUpdate,
};
use tendermint_proto::crypto::{public_key::Sum, PublicKey};
use tendermint_proto::types::Header;

pub struct ABCIProvider<T> {
    inner: T,
    pub(crate) validator_updates: Option<HashMap<[u8; 32], ValidatorUpdate>>,
}

pub struct InitChainCtx {}

impl From<RequestInitChain> for InitChainCtx {
    fn from(_: RequestInitChain) -> Self {
        InitChainCtx {}
    }
}

pub struct BeginBlockCtx {
    pub height: u64,
    pub header: Header,
}

impl From<RequestBeginBlock> for BeginBlockCtx {
    fn from(req: RequestBeginBlock) -> Self {
        let header = req.header.expect("Missing header in BeginBlock");
        let height = header.height as u64;
        BeginBlockCtx { height, header }
    }
}

#[derive(Default)]
pub struct EndBlockCtx {
    pub validator_updates: RefCell<HashMap<[u8; 32], ValidatorUpdate>>,
}

impl From<RequestEndBlock> for EndBlockCtx {
    fn from(_req: RequestEndBlock) -> Self {
        Default::default()
    }
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
    InitChain(Adapter<RequestInitChain>),
    BeginBlock(Box<Adapter<RequestBeginBlock>>), // Boxed because this variant is much larger than the others
    EndBlock(Adapter<RequestEndBlock>),
    DeliverTx(C),
    CheckTx(C),
}

impl<C> From<RequestInitChain> for ABCICall<C> {
    fn from(req: RequestInitChain) -> Self {
        ABCICall::InitChain(req.into())
    }
}

impl<C> From<RequestBeginBlock> for ABCICall<C> {
    fn from(req: RequestBeginBlock) -> Self {
        ABCICall::BeginBlock(Box::new(req.into()))
    }
}

impl<C> From<RequestEndBlock> for ABCICall<C> {
    fn from(req: RequestEndBlock) -> Self {
        ABCICall::EndBlock(req.into())
    }
}

impl<T: App> Call for ABCIProvider<T> {
    type Call = ABCICall<T::Call>;
    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.reset();
        use ABCICall::*;
        match call {
            InitChain(req) => {
                let ctx = req.into_inner().into();
                self.inner.init_chain(&ctx)?;

                Ok(())
            }
            BeginBlock(req) => {
                let ctx = req.into_inner().into();
                self.inner.begin_block(&ctx)?;

                Ok(())
            }
            EndBlock(req) => {
                let ctx = req.into_inner().into();
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
