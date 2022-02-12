use super::{NonceCall, NoncePlugin, SigType, SignerCall, SignerPlugin};
use crate::call::Call as CallTrait;
use crate::client::{AsyncCall, Client};
use crate::coins::Address;
use crate::context::{Context, GetContext};
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use std::ops::{Deref, DerefMut};

pub struct SdkCompatPlugin<T, const ID: &'static str> {
    inner: T,
}

impl<T, const ID: &'static str> Deref for SdkCompatPlugin<T, ID> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, const ID: &'static str> DerefMut for SdkCompatPlugin<T, ID> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: State, const ID: &'static str> State for SdkCompatPlugin<T, ID> {
    type Encoding = (T::Encoding,);

    fn create(store: orga::store::Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store, data.0)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<T: State, const ID: &'static str> From<SdkCompatPlugin<T, ID>> for (T::Encoding,) {
    fn from(plugin: SdkCompatPlugin<T, ID>) -> Self {
        (plugin.inner.into(),)
    }
}

pub enum Call<T> {
    Native(T),
    Sdk(sdk::Tx),
}

unsafe impl<T> Send for Call<T> {}

impl<T: Encode> Encode for Call<T> {
    fn encoding_length(&self) -> ed::Result<usize> {
        match self {
            Call::Native(native) => native.encoding_length(),
            Call::Sdk(_) => unimplemented!(),
        }
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match self {
            Call::Native(native) => native.encode_into(dest),
            Call::Sdk(_) => unimplemented!(),
        }
    }
}

impl<T: Decode> Decode for Call<T> {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        if let Some('{') = bytes.first().map(|b| *b as char) {
            let tx = serde_json::from_slice(bytes.as_slice())
                .map_err(|_| ed::Error::UnexpectedByte(123))?;
            Ok(Call::Sdk(tx))
        } else {
            let native = T::decode(bytes.as_slice())?;
            Ok(Call::Native(native))
        }
    }
}

pub mod sdk {
    use super::{Error, Result};
    use serde::{Deserialize, Serialize};

    pub struct SignBytes(pub Vec<u8>);

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Tx {
        pub msg: Vec<Msg>,
        pub fee: Fee,
        pub memo: String,
        pub signatures: Vec<Signature>,
    }

    impl Tx {
        pub fn sign_bytes(&self, chain_id: String, nonce: u64) -> Result<SignBytes> {
            let sign_tx = SignTx {
                account_number: "0".to_string(),
                chain_id,
                fee: self.fee.clone(),
                memo: self.memo.clone(),
                msgs: self.msg.clone(),
                sequence: nonce.to_string(),
            };

            let bytes = serde_json::to_vec(&sign_tx).map_err(|e| Error::App(e.to_string()))?;
            Ok(SignBytes(bytes))
        }

        pub fn pubkey(&self) -> Result<[u8; 33]> {
            let pubkey_b64 = &self
                .signatures
                .first()
                .ok_or_else(|| Error::App("No signatures provided".to_string()))?
                .pub_key
                .value;

            let pubkey_bytes = base64::decode(pubkey_b64).map_err(|e| Error::App(e.to_string()))?;

            let mut pubkey_arr = [0; 33];
            pubkey_arr.copy_from_slice(&pubkey_bytes);

            Ok(pubkey_arr)
        }

        pub fn signature(&self) -> Result<[u8; 64]> {
            let sig_b64 = &self
                .signatures
                .first()
                .ok_or_else(|| Error::App("No signatures provided".to_string()))?
                .signature;

            let sig_bytes = base64::decode(sig_b64).map_err(|e| Error::App(e.to_string()))?;

            let mut sig_arr = [0; 64];
            sig_arr.copy_from_slice(&sig_bytes);

            Ok(sig_arr)
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct SignTx {
        pub account_number: String,
        pub chain_id: String,
        pub fee: Fee,
        pub memo: String,
        pub msgs: Vec<Msg>,
        pub sequence: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Msg {
        #[serde(rename = "type")]
        pub type_: String,
        pub value: serde_json::Map<String, serde_json::Value>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Fee {
        pub amount: Vec<Coin>,
        pub gas: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Coin {
        pub amount: String,
        pub denom: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Signature {
        pub pub_key: PubKey,
        pub signature: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct PubKey {
        #[serde(rename = "type")]
        pub type_: String,
        pub value: String,
    }
}

pub struct ConvertFn<T>(fn(sdk::Msg) -> Result<T>);

impl<T> Deref for ConvertFn<T> {
    type Target = fn(sdk::Msg) -> Result<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, const ID: &'static str> CallTrait for SdkCompatPlugin<SignerPlugin<NoncePlugin<T>>, ID>
where
    T: CallTrait + State,
    T::Call: Encode + 'static,
{
    type Call = Call<SignerCall>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        Context::remove::<sdk::SignBytes>();

        let sdk_tx = match call {
            Call::Native(call) => return self.inner.call(call),
            Call::Sdk(tx) => tx,
        };

        let pubkey = sdk_tx.pubkey()?;
        let signature = sdk_tx.signature()?;
        let address = Address::from_pubkey(pubkey);
        let nonce = self.nonce(address)?;

        let sign_bytes = sdk_tx.sign_bytes(ID.to_string(), nonce)?;
        Context::add(sign_bytes);

        let convert = self
            .context::<ConvertFn<T::Call>>()
            .ok_or_else(|| Error::App("SDK call conversion function not set".into()))?;

        let mut calls = sdk_tx
            .msg
            .into_iter()
            .map(|msg| convert(msg))
            .collect::<Result<Vec<_>>>()?;

        // TODO: handle multiple calls
        let inner_call = calls
            .drain(..)
            .next()
            .ok_or_else(|| Error::App("No messages provided".into()))?;
        let nonce_call = NonceCall {
            nonce: Some(nonce),
            inner_call,
        };
        let call = SignerCall {
            signature: Some(signature),
            pubkey: Some(pubkey),
            sigtype: SigType::Sdk,
            call_bytes: nonce_call.encode()?,
        };
        self.inner.call(call)?;

        Ok(())
    }
}

impl<T: Query, const ID: &'static str> Query for SdkCompatPlugin<T, ID> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

pub struct SdkCompatAdapter<T, U> {
    inner: std::marker::PhantomData<fn(T)>,
    parent: U,
}

impl<T, U: Clone> Clone for SdkCompatAdapter<T, U> {
    fn clone(&self) -> Self {
        Self {
            inner: std::marker::PhantomData,
            parent: self.parent.clone(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: CallTrait, U> AsyncCall for SdkCompatAdapter<T, U>
where
    U: AsyncCall<Call = Call<T::Call>>,
    T::Call: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        self.parent.call(Call::Native(call)).await
    }
}

impl<T: Client<SdkCompatAdapter<T, U>>, U: Clone, const ID: &'static str> Client<U>
    for SdkCompatPlugin<T, ID>
{
    type Client = T::Client;

    fn create_client(parent: U) -> T::Client {
        T::create_client(SdkCompatAdapter {
            inner: std::marker::PhantomData,
            parent,
        })
    }
}

#[cfg(feature = "abci")]
mod abci {
    use super::super::*;
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<T, const ID: &'static str> BeginBlock for SdkCompatPlugin<T, ID>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T, const ID: &'static str> EndBlock for SdkCompatPlugin<T, ID>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T, const ID: &'static str> InitChain for SdkCompatPlugin<T, ID>
    where
        T: InitChain + State + CallTrait,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }
}
