use serde::Serialize;

use crate::call::Call as CallTrait;
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::{Address, Symbol};
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub const MAX_CALL_SIZE: usize = 65_535;

#[derive(Serialize)]
pub struct SdkCompatPlugin<S, T> {
    inner: T,
    symbol: PhantomData<S>,
}

impl<S, T> Deref for SdkCompatPlugin<S, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<S, T> DerefMut for SdkCompatPlugin<S, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<S, T: State> State for SdkCompatPlugin<S, T> {
    type Encoding = (T::Encoding,);

    fn create(store: orga::store::Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            inner: T::create(store, data.0)?,
            symbol: PhantomData,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.inner.flush()?,))
    }
}

impl<S, T: State> From<SdkCompatPlugin<S, T>> for (T::Encoding,) {
    fn from(plugin: SdkCompatPlugin<S, T>) -> Self {
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
            Call::Sdk(tx) => {
                let bytes = serde_json::to_vec(tx).map_err(|_| ed::Error::UnexpectedByte(0))?;
                Ok(bytes.len())
            }
        }
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match self {
            Call::Native(native) => native.encode_into(dest),
            Call::Sdk(tx) => {
                let bytes = serde_json::to_vec(tx).map_err(|_| ed::Error::UnexpectedByte(0))?;

                if bytes.len() > MAX_CALL_SIZE {
                    return Err(ed::Error::UnexpectedByte(0));
                }

                dest.write_all(&bytes)?;
                Ok(())
            }
        }
    }
}

impl<T: Decode> Decode for Call<T> {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        if bytes.len() > MAX_CALL_SIZE {
            return Err(ed::Error::UnexpectedByte(0));
        }

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
    use super::{Address, Error, Result};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Tx {
        pub msg: Vec<Msg>,
        pub fee: Fee,
        pub memo: String,
        pub signatures: Vec<Signature>,
    }

    impl Tx {
        pub fn sign_bytes(&self, chain_id: String, nonce: u64) -> Result<Vec<u8>> {
            let sign_tx = SignDoc {
                account_number: "0".to_string(),
                chain_id,
                fee: self.fee.clone(),
                memo: self.memo.clone(),
                msgs: self.msg.clone(),
                sequence: nonce.to_string(),
            };

            serde_json::to_vec(&sign_tx).map_err(|e| Error::App(e.to_string()))
        }

        pub fn sender_pubkey(&self) -> Result<[u8; 33]> {
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

        pub fn sender_address(&self) -> Result<Address> {
            Ok(Address::from_pubkey(self.sender_pubkey()?))
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
    pub struct SignDoc {
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
        pub value: serde_json::Value,
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

    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgSend {
        pub from_address: String,
        pub to_address: String,
        pub amount: Vec<Coin>,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgDelegate {
        pub delegator_address: String,
        pub validator_address: String,
        pub amount: Option<Coin>,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgBeginRedelegate {
        pub delegator_address: String,
        pub validator_src_address: String,
        pub validator_dst_address: String,
        pub amount: Option<Coin>,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgUndelegate {
        pub delegator_address: String,
        pub validator_address: String,
        pub amount: Option<Coin>,
    }
}

pub trait ConvertSdkTx {
    type Output;

    fn convert(&self, msg: &sdk::Tx) -> Result<Self::Output>;
}

impl<S: Symbol, T> CallTrait for SdkCompatPlugin<S, T>
where
    T: CallTrait + State + ConvertSdkTx<Output = T::Call>,
    T::Call: Encode + 'static,
{
    type Call = Call<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let call = match call {
            Call::Native(call) => call,
            Call::Sdk(tx) => self.inner.convert(&tx)?,
        };

        self.inner.call(call)
    }
}

impl<S, T: Query> Query for SdkCompatPlugin<S, T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

pub struct SdkCompatAdapter<T, U, S> {
    inner: std::marker::PhantomData<fn(T, S)>,
    parent: U,
}

impl<T, U: Clone, S> Clone for SdkCompatAdapter<T, U, S> {
    fn clone(&self) -> Self {
        Self {
            inner: std::marker::PhantomData,
            parent: self.parent.clone(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: CallTrait, U, S> AsyncCall for SdkCompatAdapter<T, U, S>
where
    U: AsyncCall<Call = Call<T::Call>>,
    T::Call: Send,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        self.parent.call(Call::Native(call)).await
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query,
        U: for<'a> AsyncQuery<Query = T::Query, Response<'a> = std::rc::Rc<SdkCompatPlugin<S, T>>>
            + Clone,
        S,
    > AsyncQuery for SdkCompatAdapter<T, U, S>
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

pub struct SdkCompatClient<T: Client<SdkCompatAdapter<T, U, S>>, U: Clone, S> {
    inner: T::Client,
    _parent: U,
}

impl<T: Client<SdkCompatAdapter<T, U, S>>, U: Clone, S> Deref for SdkCompatClient<T, U, S> {
    type Target = T::Client;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Client<SdkCompatAdapter<T, U, S>>, U: Clone, S> DerefMut for SdkCompatClient<T, U, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

impl<
        T: Client<SdkCompatAdapter<T, U, S>> + CallTrait,
        U: Clone + AsyncCall<Call = Call<T::Call>>,
        S,
    > SdkCompatClient<T, U, S>
{
    #[cfg(target_arch = "wasm32")]
    pub async fn send_sdk_tx(
        &mut self,
        sign_doc: sdk::SignDoc,
    ) -> std::result::Result<(), JsValue> {
        let signer = crate::plugins::signer::keplr::Signer;
        let sig = signer.sign_sdk(sign_doc.clone()).await?;

        let tx = sdk::Tx {
            msg: sign_doc.msgs,
            signatures: vec![sig],
            fee: sign_doc.fee,
            memo: sign_doc.memo,
        };
        self._parent
            .call(Call::Sdk(tx))
            .await
            .map_err(|e| e.to_string().into())
    }
}

impl<S, T: Client<SdkCompatAdapter<T, U, S>>, U: Clone> Client<U> for SdkCompatPlugin<S, T> {
    type Client = SdkCompatClient<T, U, S>;

    fn create_client(parent: U) -> Self::Client {
        SdkCompatClient {
            inner: T::create_client(SdkCompatAdapter {
                inner: std::marker::PhantomData,
                parent: parent.clone(),
            }),
            _parent: parent,
        }
    }
}

#[cfg(feature = "abci")]
mod abci {
    use super::super::*;
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<S, T> BeginBlock for SdkCompatPlugin<S, T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<S, T> EndBlock for SdkCompatPlugin<S, T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<S, T> InitChain for SdkCompatPlugin<S, T>
    where
        T: InitChain + State + CallTrait,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }
}
