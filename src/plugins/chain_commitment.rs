use super::GetNonce;
use super::{sdk_compat::sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call as CallTrait;
use crate::context::Context;
use crate::describe::{Describe, Descriptor};
use crate::encoding::{Decode, Encode};
use crate::migrate::{MigrateFrom, MigrateInto};
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use std::marker::PhantomData;
use std::ops::Deref;

#[derive(Encode, Decode, Default, State)]
#[state(transparent)]
pub struct ChainCommitmentPlugin<T, const ID: &'static str> {
    pub inner: T,
}

impl<T: Describe, const ID: &'static str> Describe for ChainCommitmentPlugin<T, ID> {
    fn describe() -> Descriptor {
        T::describe()
    }
}

impl<T, const ID: &'static str> GetNonce for ChainCommitmentPlugin<T, ID>
where
    T: GetNonce,
{
    fn nonce(&self, address: crate::coins::Address) -> Result<u64> {
        self.inner.nonce(address)
    }
}

impl<T1, T2, const ID1: &'static str, const ID2: &'static str>
    MigrateFrom<ChainCommitmentPlugin<T1, ID1>> for ChainCommitmentPlugin<T2, ID2>
where
    T1: MigrateInto<T2>,
{
    fn migrate_from(other: ChainCommitmentPlugin<T1, ID1>) -> Result<Self> {
        Ok(Self {
            inner: other.inner.migrate_into()?,
        })
    }
}

pub struct ChainId(pub &'static str);

impl Deref for ChainId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: CallTrait, const ID: &'static str> CallTrait for ChainCommitmentPlugin<T, ID> {
    type Call = Vec<u8>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        Context::add(ChainId(ID));
        let expected_id: Vec<u8> = ID.bytes().collect();
        if call.len() < expected_id.len() {
            return Err(Error::App("Invalid chain ID length".into()));
        }
        let chain_id = &call[..expected_id.len()];
        if chain_id != expected_id {
            return Err(Error::App(format!(
                "Invalid chain ID (expected {}, got {})",
                ID,
                String::from_utf8(chain_id.to_vec()).unwrap_or_default()
            )));
        }
        dbg!(&call);
        dbg!(&call[chain_id.len()..]);
        let inner_call = Decode::decode(&call[chain_id.len()..])?;
        dbg!(&inner_call);
        self.inner.call(inner_call)
    }
}

impl<T: Query, const ID: &'static str> Query for ChainCommitmentPlugin<T, ID> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<T, const ID: &'static str> ConvertSdkTx for ChainCommitmentPlugin<T, ID>
where
    T: ConvertSdkTx<Output = T::Call> + CallTrait,
{
    type Output = Vec<u8>;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<Vec<u8>> {
        Context::add(ChainId(ID));
        let id_bytes = ID.as_bytes();
        let inner_call = self.inner.convert(sdk_tx)?;

        let mut call_bytes = Vec::with_capacity(id_bytes.len() + inner_call.encoding_length()?);
        call_bytes.extend_from_slice(id_bytes);
        inner_call.encode_into(&mut call_bytes)?;

        Ok(call_bytes)
    }
}

// impl<T, const ID: &'static str> Describe for ChainCommitmentPlugin<T, ID>
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

    impl<T, const ID: &'static str> BeginBlock for ChainCommitmentPlugin<T, ID>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T, const ID: &'static str> EndBlock for ChainCommitmentPlugin<T, ID>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T, const ID: &'static str> InitChain for ChainCommitmentPlugin<T, ID>
    where
        T: InitChain + State,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }

    impl<T, const ID: &'static str> crate::abci::AbciQuery for ChainCommitmentPlugin<T, ID>
    where
        T: crate::abci::AbciQuery + State,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::abci::RequestQuery,
        ) -> Result<tendermint_proto::abci::ResponseQuery> {
            self.inner.abci_query(request)
        }
    }
}
