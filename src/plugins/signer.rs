use crate::call::Call;
use crate::client::{AsyncCall, Client};
use crate::coins::Address;
use crate::context::Context;
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
    pub sdk_compat: bool,
    pub call_bytes: Vec<u8>,
}

use serde::Serialize;

#[derive(Serialize)]
struct SdkCompatMsg {
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

fn sdk_compat_wrap(call_bytes: &[u8], address: Address) -> Result<Vec<u8>> {
    let data_b64 = base64::encode(call_bytes);
    let msg = SdkCompatMsg {
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

impl SignerCall {
    fn verify(&self) -> Result<Option<Address>> {
        match (self.pubkey, self.signature) {
            (Some(pubkey_bytes), Some(signature)) => {
                use secp256k1::hashes::sha256;
                let secp = Secp256k1::verification_only();
                let pubkey = PublicKey::from_slice(&pubkey_bytes)?;
                let address = Address::from_pubkey(pubkey_bytes);

                let bytes = if self.sdk_compat {
                    sdk_compat_wrap(self.call_bytes.as_slice(), address)?
                } else {
                    self.call_bytes.to_vec()
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

impl<T: Call> Call for SignerPlugin<T> {
    type Call = SignerCall;
    fn call(&mut self, call: Self::Call) -> Result<()> {
        Context::remove::<Signer>();
        let signer_ctx = Signer {
            signer: call.verify()?,
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
                sdk_compat: false,
            })
            .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn call(&mut self, call: Self::Call) -> Result<()> {
        let call_bytes = Encode::encode(&call)?;
        let call_hex = hex::encode(call_bytes.as_slice());
        web_sys::console::log_1(&format!("call: {}", call_hex).into());

        let signature = self.signer.sign(call_bytes.as_slice()).await;
        let pubkey = self.signer.pubkey().await;

        self.parent
            .call(SignerCall {
                call_bytes,
                pubkey: Some(pubkey),
                signature: Some(signature),
                sdk_compat: true,
            })
            .await
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

#[cfg(target_arch = "wasm32")]
pub mod keplr {
    use js_sys::{
        Array, Function, Object, Promise,
        Reflect::{apply, get},
        Uint8Array,
    };
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    // TODO: this should be specified by consumer, not hardcoded here
    const CHAIN_ID: &str = "nomic-stakenet";

    pub struct Signer {
        handle: Option<KeplrHandle>,
    }

    pub struct KeplrHandle {
        keplr: Object,
        signer: JsValue,
    }

    impl KeplrHandle {
        pub fn new() -> Self {
            unsafe {
                let window = web_sys::window().expect("no global `window` exists");
                let keplr = window.get("keplr").expect("no `keplr` in global `window`");

                let args = Array::new();
                // TODO: get chainid from somewhere
                Array::push(&args, &CHAIN_ID.to_string().into());
                let get_offline_signer: Function =
                    get(&keplr, &"getOfflineSigner".to_string().into())
                        .unwrap()
                        .into();
                let signer = apply(&get_offline_signer, &keplr, &args).unwrap();

                Self { keplr, signer }
            }
        }
    }

    impl Signer {
        pub fn new() -> Self {
            Self { handle: None }
        }

        fn handle(&mut self) -> &KeplrHandle {
            if self.handle.is_none() {
                self.handle = Some(KeplrHandle::new());
            }

            self.handle.as_ref().unwrap()
        }

        pub async fn pubkey(&mut self) -> [u8; 33] {
            unsafe {
                let get_accounts: Function = get(&self.handle().signer, &"getAccounts".to_string().into())
                    .unwrap()
                    .into();
                let accounts_promise: Promise = apply(&get_accounts, &self.handle().signer, &Array::new())
                    .unwrap()
                    .into();
                let accounts = JsFuture::from(accounts_promise).await.unwrap();
                let account = get(&accounts, &0i32.into()).unwrap();
                let pubkey: Uint8Array =
                    get(&account, &"pubkey".to_string().into()).unwrap().into();
                let pubkey_vec = pubkey.to_vec();
                let mut pubkey_arr = [0u8; 33];
                pubkey_arr.copy_from_slice(&pubkey_vec);
                pubkey_arr
            }
        }

        pub async fn address(&mut self) -> String {
            unsafe {
                let get_accounts: Function = get(&self.handle().signer, &"getAccounts".to_string().into())
                    .unwrap()
                    .into();
                let accounts_promise: Promise = apply(&get_accounts, &self.handle().signer, &Array::new())
                    .unwrap()
                    .into();
                let accounts = JsFuture::from(accounts_promise).await.unwrap();
                let account = get(&accounts, &0i32.into()).unwrap();
                get(&account, &"address".to_string().into())
                    .unwrap()
                    .as_string()
                    .unwrap()
            }
        }

        pub async fn sign(&mut self, call_bytes: &[u8]) -> [u8; 64] {
            unsafe {
                let msg = Array::new();
                for byte in call_bytes {
                    Array::push(&msg, &(*byte as i32).into());
                }

                let args = Array::new();
                // TOOD: get chainid from somewhere
                Array::push(&args, &CHAIN_ID.to_string().into());
                Array::push(&args, &self.address().await.into());
                Array::push(&args, &msg.into());

                let sign_arbitrary: Function = get(&self.handle().keplr, &"signArbitrary".to_string().into())
                    .unwrap()
                    .into();
                let sign_promise: Promise =
                    apply(&sign_arbitrary, &self.handle().keplr, &args).unwrap().into();
                let res = JsFuture::from(sign_promise).await.unwrap();

                let signature_b64: String = get(&res, &"signature".to_string().into())
                    .unwrap()
                    .as_string()
                    .unwrap();
                let signature_vec = base64::decode(&signature_b64).unwrap();
                let mut signature_arr = [0u8; 64];
                signature_arr.copy_from_slice(&signature_vec);
                signature_arr
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_privkey() -> Result<SecretKey> {
    // Ensure orga home directory exists
    let orga_home = home::home_dir()
        .expect("No home directory set")
        .join(".orga");

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
