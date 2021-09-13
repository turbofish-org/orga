use super::{BeginBlockCtx, Context, EndBlockCtx, InitChainCtx};
use crate::abci::{BeginBlock, EndBlock, InitChain};
use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::Result;
use tendermint::public_key::Ed25519;
use tendermint::signature::Ed25519Signature;

pub struct SignerProvider<T> {
    inner: T,
}

pub struct Signer {
    pub signer: Option<[u8; 32]>,
}

#[derive(Encode, Decode)]
pub struct SignerCall {
    pub signature: Option<[u8; 64]>,
    pub pubkey: Option<[u8; 32]>,
    pub call_bytes: Vec<u8>,
}

impl SignerCall {
    fn verify(&self) -> Result<Option<[u8; 32]>> {
        match (self.pubkey, self.signature) {
            (Some(pubkey_bytes), Some(signature)) => {
                let pubkey = Ed25519::from_bytes(&pubkey_bytes)?;
                let signature = Ed25519Signature::new(signature);
                pubkey.verify_strict(&self.call_bytes, &signature)?;

                Ok(Some(pubkey_bytes))
            }
            (None, None) => Ok(None),
            _ => failure::bail!("Malformed transaction"),
        }
    }
}

impl<T: Call + State> Call for SignerProvider<T> {
    type Call = SignerCall;
    fn call(&mut self, call: Self::Call) -> Result<()> {
        let signer_ctx = Signer {
            signer: call.verify()?,
        };
        Context::add(signer_ctx);
        let inner_call = Decode::decode(call.call_bytes.as_slice())?;

        self.inner.call(inner_call)
    }
}

impl<T: Query> Query for SignerProvider<T> {
    type Query = T::Query;
    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<T> State for SignerProvider<T>
where
    T: State,
    T::Encoding: From<T>,
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

impl<T> From<SignerProvider<T>> for (T::Encoding,)
where
    T: State,
    T::Encoding: From<T>,
{
    fn from(provider: SignerProvider<T>) -> Self {
        (provider.inner.into(),)
    }
}

// TODO: In the future, Signer shouldn't need to know about ABCI, but
// implementing passthrough of ABCI lifecycle methods as below seems preferable to creating a formal
// distinction between Contexts and normal State / Call / Query types for now.
impl<T> BeginBlock for SignerProvider<T>
where
    T: BeginBlock + State,
{
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.inner.begin_block(ctx)
    }
}

impl<T> EndBlock for SignerProvider<T>
where
    T: EndBlock + State,
{
    fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
        self.inner.end_block(ctx)
    }
}

impl<T> InitChain for SignerProvider<T>
where
    T: InitChain + State,
{
    fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
        self.inner.init_chain(ctx)
    }
}
