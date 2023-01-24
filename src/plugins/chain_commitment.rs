use serde::{Deserialize, Serialize};

use super::{sdk_compat::sdk::Tx as SdkTx, ConvertSdkTx};
use crate::call::Call as CallTrait;
use crate::client::{AsyncCall, AsyncQuery, Client as ClientTrait};
use crate::context::Context;
use crate::encoding::{Decode, Encode};
use crate::migrate::MigrateFrom;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use std::marker::PhantomData;
use std::ops::Deref;

#[derive(Encode, Decode, Default, Serialize, Deserialize, MigrateFrom)]
#[serde(transparent)]
pub struct ChainCommitmentPlugin<T, const ID: &'static str> {
    inner: T,
}

pub struct ChainId(pub &'static str);

impl Deref for ChainId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
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

impl<T, const ID: &'static str> ConvertSdkTx for ChainCommitmentPlugin<T, ID>
where
    T: ConvertSdkTx<Output = T::Call> + CallTrait,
{
    type Output = Vec<u8>;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<Vec<u8>> {
        let id_bytes = ID.as_bytes();
        let inner_call = self.inner.convert(sdk_tx)?;

        let mut call_bytes = Vec::with_capacity(id_bytes.len() + inner_call.encoding_length()?);
        call_bytes.extend_from_slice(id_bytes);
        inner_call.encode_into(&mut call_bytes)?;

        Ok(call_bytes)
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
impl<T: CallTrait, U: AsyncCall<Call = Vec<u8>> + Clone, const ID: &'static str> AsyncCall
    for Client<T, U, ID>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        let id_bytes = ID.as_bytes();

        let mut call_bytes = Vec::with_capacity(id_bytes.len() + call.encoding_length()?);
        call_bytes.extend_from_slice(id_bytes);
        call.encode_into(&mut call_bytes)?;

        self.parent.call(call_bytes).await
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query + 'static,
        U: for<'a> AsyncQuery<
                Query = T::Query,
                Response<'a> = std::rc::Rc<ChainCommitmentPlugin<T, ID>>,
            > + Clone,
        const ID: &'static str,
    > AsyncQuery for Client<T, U, ID>
{
    type Query = T::Query;
    type Response<'a> = std::rc::Rc<T>;

    async fn query<F, R>(&self, query: Self::Query, mut check: F) -> Result<R>
    where
        F: FnMut(Self::Response<'_>) -> Result<R>,
    {
        self.parent
            .query(query, |plugin| {
                check(std::rc::Rc::new(
                    std::rc::Rc::try_unwrap(plugin)
                        .map_err(|_| ())
                        .unwrap()
                        .inner,
                ))
            })
            .await
    }
}

impl<T: ClientTrait<Client<T, U, ID>>, U: Clone, const ID: &'static str> ClientTrait<U>
    for ChainCommitmentPlugin<T, ID>
{
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
    fn attach(&mut self, store: Store) -> Result<()> {
        self.inner.attach(store)
    }

    fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
        self.inner.flush(out)
    }

    fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
        Context::add(ChainId(ID));
        let inner = T::load(store, bytes)?;
        Ok(Self { inner })
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
