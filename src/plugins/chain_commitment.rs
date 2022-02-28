use crate::call::Call as CallTrait;
use crate::client::{AsyncCall, Client as ClientTrait};
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use std::marker::PhantomData;
use std::ops::Deref;

pub struct ChainCommitmentPlugin<T, const ID: &'static str> {
    inner: T,
}

impl<T, const ID: &'static str> Deref for ChainCommitmentPlugin<T, ID> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: CallTrait, const ID: &'static str> CallTrait for ChainCommitmentPlugin<T, ID> {
    type Call = Vec<u8>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let expected_id: Vec<u8> = ID.bytes().collect();
        let chain_id = &call[..expected_id.len()];
        if chain_id != expected_id {
            return Err(Error::App("Invalid chain ID".into()));
        }

        let inner_call = Decode::decode(&call[chain_id.len()..])?;
        self.inner.call(inner_call)
    }
}

impl<T: Query, const ID: &'static str> Query for ChainCommitmentPlugin<T, ID> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

pub struct Client<T, U: Clone, const ID: &'static str> {
    parent: U,
    marker: std::marker::PhantomData<fn() -> T>,
}

impl<T, U: Clone, const ID: &'static str> Clone for Client<T, U, ID> {
    fn clone(&self) -> Self {
        Client {
            parent: self.parent.clone(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T, U: Clone + Send, const ID: &'static str> Send for Client<T, U, ID> {}

#[async_trait::async_trait(?Send)]
impl<T: CallTrait, U: AsyncCall<Call = Vec<u8>> + Clone, const ID: &'static str> AsyncCall for Client<T, U, ID>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        let id_bytes = ID.as_bytes();

        let mut call_bytes = Vec::with_capacity(id_bytes.len() + call.encoding_length()?);
        call_bytes.extend_from_slice(id_bytes);
        call.encode_into(&mut call_bytes)?;

        self.parent.call(call_bytes).await
    }
}

impl<T: ClientTrait<Client<T, U, ID>>, U: Clone, const ID: &'static str> ClientTrait<U> for ChainCommitmentPlugin<T, ID> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(Client {
            parent,
            marker: std::marker::PhantomData,
        })
    }
}

impl<T, const ID: &'static str> State for ChainCommitmentPlugin<T, ID>
where
    T: State,
{
    type Encoding = (T::Encoding,);

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store, data.0)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<T, const ID: &'static str> From<ChainCommitmentPlugin<T, ID>> for (T::Encoding,)
where
    T: State,
{
    fn from(provider: ChainCommitmentPlugin<T, ID>) -> Self {
        (provider.inner.into(),)
    }
}

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
}
