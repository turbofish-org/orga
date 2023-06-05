use orga_macros::orga;

use super::GetNonce;
use serde::Serialize;

use super::{sdk_compat::sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call as CallTrait;
use crate::context::Context;
use crate::describe::{Describe, Descriptor};
use crate::encoding::{Decode, Encode};
use crate::migrate::{MigrateFrom, MigrateInto};
use crate::prelude::LengthVec;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use std::ops::Deref;

#[orga(skip(Call, MigrateFrom))]
pub struct ChainCommitmentPlugin<T> {
    pub chain_id: LengthVec<u8, u8>,
    pub inner: T,
}

impl<T> GetNonce for ChainCommitmentPlugin<T>
where
    T: GetNonce,
{
    fn nonce(&self, address: crate::coins::Address) -> Result<u64> {
        self.inner.nonce(address)
    }
}

impl<T1, T2> MigrateFrom<ChainCommitmentPlugin<T1>> for ChainCommitmentPlugin<T2>
where
    T1: MigrateInto<T2>,
{
    fn migrate_from(other: ChainCommitmentPlugin<T1>) -> Result<Self> {
        // TODO: will have to migrate from old versions which don't store a chain id
        Ok(Self {
            chain_id: other.chain_id,
            inner: other.inner.migrate_into()?,
        })
    }
}

pub struct ChainId(pub String);

impl Deref for ChainId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl<T: CallTrait> CallTrait for ChainCommitmentPlugin<T> {
    type Call = Vec<u8>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        if self.chain_id.len() == 0 {
            return Err(Error::App("Chain ID not set".into()));
        }

        if call.len() < self.chain_id.len() {
            return Err(Error::App("Invalid chain ID length".into()));
        }

        let chain_id = &call[..self.chain_id.len()];
        if chain_id != self.chain_id.as_slice() {
            return Err(Error::App(format!(
                "Invalid chain ID (expected {}, got {})",
                String::from_utf8(self.chain_id.to_vec()).unwrap(),
                String::from_utf8(chain_id.to_vec()).unwrap_or_default()
            )));
        }

        let inner_call = Decode::decode(&call[self.chain_id.len()..])?;
        Context::add(ChainId(String::from_utf8(self.chain_id.to_vec()).unwrap()));
        self.inner.call(inner_call)
    }
}

impl<T> ConvertSdkTx for ChainCommitmentPlugin<T>
where
    T: ConvertSdkTx<Output = T::Call> + CallTrait,
{
    type Output = Vec<u8>;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<Vec<u8>> {
        Context::add(ChainId(String::from_utf8(self.chain_id.to_vec()).unwrap()));

        let id_bytes = self.chain_id.as_slice();
        let inner_call = self.inner.convert(sdk_tx)?;

        let mut call_bytes = Vec::with_capacity(id_bytes.len() + inner_call.encoding_length()?);
        call_bytes.extend_from_slice(id_bytes);
        inner_call.encode_into(&mut call_bytes)?;

        Ok(call_bytes)
    }
}

// impl<T> Describe for ChainCommitmentPlugin<T>
// where
//     T: State + Describe + 'static,
// {
//     fn describe() -> crate::describe::Descriptor {
//         crate::describe::Builder::new::<Self>()
//             .named_child::<T>("inner", &[], |v| {
//                 crate::describe::Builder::access(v, |v: Self| v.inner)
//             })
//             .build()
//     }
// }

// TODO: In the future, this plugin shouldn't need to know about ABCI, but
// implementing passthrough of ABCI lifecycle methods as below seems preferable
// to creating a formal distinction between Contexts and normal State / Call /
// Query types for now.
#[cfg(feature = "abci")]
mod abci {
    use super::super::{BeginBlockCtx, EndBlockCtx, InitChainCtx};
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<T> BeginBlock for ChainCommitmentPlugin<T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T> EndBlock for ChainCommitmentPlugin<T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T> InitChain for ChainCommitmentPlugin<T>
    where
        T: InitChain + State,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.chain_id = Context::resolve::<ChainId>()
                .ok_or_else(|| Error::App("Chain ID context not set".into()))?
                .0
                .as_bytes()
                .to_vec()
                .try_into()?;

            self.inner.init_chain(ctx)
        }
    }

    impl<T> crate::abci::AbciQuery for ChainCommitmentPlugin<T>
    where
        T: crate::abci::AbciQuery + State,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::v0_34::abci::RequestQuery,
        ) -> Result<tendermint_proto::v0_34::abci::ResponseQuery> {
            self.inner.abci_query(request)
        }
    }
}
