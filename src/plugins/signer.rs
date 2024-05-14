use super::{
    sdk_compat::{self, sdk::Tx as SdkTx, ConvertSdkTx},
    ChainId, GetNonce,
};
use crate::call::Call;
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::Address;
use crate::context::{Context, GetContext};
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1, SecretKey};
use std::ops::Deref;

pub struct SignerPlugin<T> {
    inner: T,
}

impl<T> Deref for SignerPlugin<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct Signer {
    pub signer: Option<Address>,
}

#[derive(Encode, Decode)]
pub struct SignerCall {
    pub signature: Option<[u8; 64]>,
    pub pubkey: Option<[u8; 33]>,
    pub sigtype: SigType,
    pub call_bytes: Vec<u8>,
}

#[derive(Encode, Decode)]
pub enum SigType {
    Native,
    Adr36,
    #[skip]
    Sdk(sdk_compat::sdk::Tx),
}

use serde::Serialize;

#[derive(Serialize)]
struct Adr36Msg {
    pub account_number: String,
    pub chain_id: String,
    pub fee: Fee,
    pub memo: String,
    pub msgs: [SignMsg; 1],
    pub sequence: String,
}

#[derive(Serialize)]
struct Fee {
    pub amount: [u8; 0],
    pub gas: String,
}

#[derive(Serialize)]
struct SignMsg {
    #[serde(rename = "type")]
    pub type_: String,
    pub value: Value,
}

#[derive(Serialize)]
struct Value {
    pub data: String,
    pub signer: String,
}

fn adr36_bytes(call_bytes: &[u8], address: Address) -> Result<Vec<u8>> {
    let data_b64 = base64::encode(call_bytes);
    let msg = Adr36Msg {
        chain_id: "".to_string(),
        account_number: "0".to_string(),
        sequence: "0".to_string(),
        fee: Fee {
            gas: "0".to_string(),
            amount: [0; 0],
        },
        msgs: [SignMsg {
            type_: "sign/MsgSignData".to_string(),
            value: Value {
                signer: address.to_string(),
                data: data_b64,
            },
        }],
        memo: "".to_string(),
    };
    serde_json::to_vec(&msg).map_err(|e| Error::App(format!("{}", e)))
}

impl<T: State, U> SignerPlugin<T>
where
    T: Deref<Target = U>,
    U: GetNonce,
{
    fn sdk_sign_bytes(&mut self, tx: &SdkTx, address: Address) -> Result<Vec<u8>> {
        let nonce = self.inner.nonce(address)? + 1;
        let chain_id = self
            .context::<ChainId>()
            .ok_or_else(|| Error::App("Chain ID not found".to_string()))?
            .deref()
            .to_string();
        tx.sign_bytes(chain_id, nonce)
    }

    fn verify(&mut self, call: &SignerCall) -> Result<Option<Address>> {
        match (call.pubkey, call.signature) {
            (Some(pubkey_bytes), Some(signature)) => {
                use secp256k1::hashes::sha256;
                let secp = Secp256k1::verification_only();
                let pubkey = PublicKey::from_slice(&pubkey_bytes)?;
                let address = Address::from_pubkey(pubkey_bytes);

                let bytes = match &call.sigtype {
                    SigType::Native => call.call_bytes.to_vec(),
                    SigType::Adr36 => adr36_bytes(call.call_bytes.as_slice(), address)?,
                    SigType::Sdk(tx) => self.sdk_sign_bytes(tx, address)?,
                };

                let msg = Message::from_hashed_data::<sha256::Hash>(bytes.as_slice());
                let signature = Signature::from_compact(&signature)?;
                #[cfg(not(fuzzing))]
                secp.verify_ecdsa(&msg, &signature, &pubkey)?;

                Ok(Some(address))
            }
            (None, None) => Ok(None),
            _ => Err(Error::Signer("Malformed transaction".into())),
        }
    }
}

impl<T: Call + State, U> Call for SignerPlugin<T>
where
    T: Deref<Target = U>,
    U: GetNonce,
{
    type Call = SignerCall;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        Context::remove::<Signer>();
        let signer_ctx = Signer {
            signer: self.verify(&call)?,
        };
        Context::add(signer_ctx);

        let inner_call = Decode::decode(call.call_bytes.as_slice())?;
        self.inner.call(inner_call)
    }
}

impl<T: Query> Query for SignerPlugin<T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<T> ConvertSdkTx for SignerPlugin<T>
where
    T: State + ConvertSdkTx<Output = T::Call> + Call,
{
    type Output = SignerCall;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<SignerCall> {
        let signature = sdk_tx.signature()?;
        let pubkey = sdk_tx.sender_pubkey()?;
        let inner_call = self.inner.convert(sdk_tx)?.encode()?;

        Ok(SignerCall {
            signature: Some(signature),
            pubkey: Some(pubkey),
            sigtype: SigType::Sdk(sdk_tx.clone()),
            call_bytes: inner_call,
        })
    }
}

pub struct SignerClient<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<fn() -> T>,
    #[cfg(not(target_arch = "wasm32"))]
    privkey: SecretKey,
    #[cfg(target_arch = "wasm32")]
    signer: keplr::Signer,
}

impl<T, U: Clone> Clone for SignerClient<T, U> {
    fn clone(&self) -> Self {
        SignerClient {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
            #[cfg(not(target_arch = "wasm32"))]
            privkey: SecretKey::from_slice(&self.privkey.serialize_secret()).unwrap(),
            #[cfg(target_arch = "wasm32")]
            signer: keplr::Signer::new(),
        }
    }
}

unsafe impl<T, U: Clone + Send> Send for SignerClient<T, U> {}

#[async_trait::async_trait(?Send)]
impl<T: Call, U: AsyncCall<Call = SignerCall> + Clone> AsyncCall for SignerClient<T, U>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    #[cfg(not(target_arch = "wasm32"))]
    async fn call(&mut self, call: Self::Call) -> Result<()> {
        use secp256k1::hashes::sha256;
        let secp = Secp256k1::signing_only();
        let call_bytes = Encode::encode(&call)?;
        let msg = Message::from_hashed_data::<sha256::Hash>(&call_bytes);
        let signature = secp.sign_ecdsa(&msg, &self.privkey).serialize_compact();
        let pubkey = PublicKey::from_secret_key(&secp, &self.privkey);
        let pubkey = pubkey.serialize();

        self.parent
            .call(SignerCall {
                call_bytes,
                pubkey: Some(pubkey),
                signature: Some(signature),
                sigtype: SigType::Native,
            })
            .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn call(&mut self, call: Self::Call) -> Result<()> {
        let call_bytes = Encode::encode(&call)?;
        let call_hex = hex::encode(call_bytes.as_slice());

        let signature = self.signer.sign(call_bytes.as_slice()).await;
        let pubkey = self.signer.pubkey().await;

        self.parent
            .call(SignerCall {
                call_bytes,
                pubkey: Some(pubkey),
                signature: Some(signature),
                sigtype: SigType::Adr36,
            })
            .await
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Query, U: AsyncQuery<Query = T::Query, Response = SignerPlugin<T>> + Clone> AsyncQuery
    for SignerClient<T, U>
{
    type Query = T::Query;
    type Response = T;

    async fn query<F, R>(&self, query: Self::Query, mut check: F) -> Result<R>
    where
        F: FnMut(Self::Response) -> Result<R>,
    {
        self.parent.query(query, |plugin| check(plugin.inner)).await
    }
}

impl<T: Client<SignerClient<T, U>>, U: Clone> Client<U> for SignerPlugin<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(SignerClient {
            parent,
            marker: std::marker::PhantomData,
            #[cfg(not(target_arch = "wasm32"))]
            privkey: load_privkey().expect("Failed to load private key"),
            #[cfg(target_arch = "wasm32")]
            signer: keplr::Signer::new(),
        })
    }
}

impl<T> State for SignerPlugin<T>
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

impl<T> From<SignerPlugin<T>> for (T::Encoding,)
where
    T: State,
{
    fn from(provider: SignerPlugin<T>) -> Self {
        (provider.inner.into(),)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_privkey() -> Result<SecretKey> {
    // Ensure orga home directory exists
    let orga_home = std::path::PathBuf::new();

    std::fs::create_dir_all(&orga_home)?;
    let keypair_path = orga_home.join("privkey");
    if keypair_path.exists() {
        // Load existing key
        let bytes = std::fs::read(&keypair_path)?;
        Ok(SecretKey::from_slice(bytes.as_slice())?)
    } else {
        // Create and save a new key
        let mut rng = secp256k1::rand::thread_rng();
        let privkey = SecretKey::new(&mut rng);
        std::fs::write(&keypair_path, privkey.serialize_secret())?;
        Ok(privkey)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct KeyPair {
    pub private: SecretKey,
    pub public: PublicKey,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_keypair() -> Result<KeyPair> {
    let secp = Secp256k1::new();
    let privkey = load_privkey()?;
    let pubkey = PublicKey::from_secret_key(&secp, &privkey);

    Ok(KeyPair {
        private: privkey,
        public: pubkey,
    })
}

// TODO: In the future, Signer shouldn't need to know about ABCI, but
// implementing passthrough of ABCI lifecycle methods as below seems preferable to creating a formal
// distinction between Contexts and normal State / Call / Query types for now.
#[cfg(feature = "abci")]
mod abci {
    use super::super::{BeginBlockCtx, EndBlockCtx, InitChainCtx};
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<T> BeginBlock for SignerPlugin<T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T> EndBlock for SignerPlugin<T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T> InitChain for SignerPlugin<T>
    where
        T: InitChain + State,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::call::Call;
//     use crate::contexts::GetContext;
//     use crate::state::State;

//     #[derive(State, Clone)]
//     struct Counter {
//         pub count: u64,
//         pub last_signer: Option<Address>,
//     }

//     impl Counter {
//         fn increment(&mut self) -> Result<()> {
//             self.count += 1;
//             let signer = self.context::<Signer>().unwrap().signer.unwrap();
//             self.last_signer.replace(signer);

//             Ok(())
//         }
//     }

//     #[derive(Encode, Decode)]
//     pub enum CounterCall {
//         Increment,
//     }

//     impl Call for Counter {
//         type Call = CounterCall;

//         fn call(&mut self, call: Self::Call) -> Result<()> {
//             match call {
//                 CounterCall::Increment => self.increment(),
//             }
//         }
//     }

//     #[derive(Clone)]
//     struct CounterClient<T> {
//         parent: T,
//     }

//     impl<T: Call<Call = CounterCall> + Clone> CounterClient<T> {
//         pub fn increment(&mut self) -> Result<()> {
//             self.parent.call(CounterCall::Increment)
//         }
//     }

//     impl<T: Clone> Client<T> for Counter {
//         type Client = CounterClient<T>;

//         fn create_client(parent: T) -> Self::Client {
//             CounterClient { parent }
//         }
//     }

// #[test]
// fn signed_increment() {
//     let state = Rc::new(RefCell::new(SignerProvider {
//         inner: Counter {
//             count: 0,
//             last_signer: None,
//         },
//     }));
//     let mut client = SignerProvider::<Counter>::create_client(state.clone());
//     client.increment().unwrap();
//     assert_eq!(state.borrow().inner.count, 1);
//     let pub_key = load_keypair().unwrap().public.to_bytes();
//     assert_eq!(state.borrow().inner.last_signer, Some(pub_key));
// }
// }
