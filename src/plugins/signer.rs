use super::{
    sdk_compat::{self, sdk::Tx as SdkTx, ConvertSdkTx},
    ChainId, GetNonce,
};
use crate::client::{AsyncCall, AsyncQuery, Client};
use crate::coins::{Address, Symbol};
use crate::context::{Context, GetContext};
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::MigrateFrom;
use crate::orga;
use crate::query::Query;
use crate::state::State;
use crate::{call::Call, migrate::MigrateInto};
use crate::{Error, Result};
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1, SecretKey};
use serde::Serialize;
use std::ops::Deref;

#[derive(Default, Encode, Decode, State)]
#[state(transparent)]
pub struct SignerPlugin<T> {
    pub inner: T,
}
impl<T> Describe for SignerPlugin<T>
where
    T: Describe,
{
    fn describe() -> crate::describe::Descriptor {
        T::describe()
    }
}

impl<T1, T2> MigrateFrom<SignerPlugin<T1>> for SignerPlugin<T2>
where
    T1: MigrateInto<T2>,
{
    fn migrate_from(other: SignerPlugin<T1>) -> Result<Self> {
        Ok(Self {
            inner: other.inner.migrate_into()?,
        })
    }
}

pub struct Signer {
    pub signer: Option<Address>,
}

#[derive(Debug, Encode, Decode)]
pub struct SignerCall {
    pub signature: Option<[u8; 64]>,
    pub pubkey: Option<[u8; 33]>,
    pub sigtype: SigType,
    pub call_bytes: Vec<u8>,
}

impl SignerCall {
    pub fn address(&self) -> Result<Address> {
        let pubkey_bytes = self
            .pubkey
            .ok_or_else(|| Error::Signer("No pubkey specified".to_string()))?;
        match &self.sigtype {
            SigType::EthPersonalSign(_) => {
                let pubkey = PublicKey::from_slice(pubkey_bytes.as_slice())?;
                let pubkey_bytes = pubkey.serialize_uncompressed();
                let mut eth_pubkey = [0; 64];
                eth_pubkey.copy_from_slice(&pubkey_bytes[1..]);
                Ok(Address::from_pubkey_eth(eth_pubkey))
            }
            _ => Ok(Address::from_pubkey(pubkey_bytes)),
        }
    }
}

#[derive(Debug, Encode, Decode)]
pub enum SigType {
    Native,
    Adr36,
    #[skip]
    Sdk(Box<sdk_compat::sdk::Tx>),
    #[skip]
    EthPersonalSign(Box<sdk_compat::sdk::Tx>),
}

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

impl<T: State> SignerPlugin<T>
where
    T: GetNonce,
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
        match (call.pubkey.as_ref(), call.signature) {
            (Some(pubkey_bytes), Some(signature)) => {
                use secp256k1::hashes::sha256;
                let secp = Secp256k1::verification_only();
                let pubkey = PublicKey::from_slice(pubkey_bytes.as_slice())?;

                let (msg, addr) = match &call.sigtype {
                    SigType::Native => {
                        let addr = Address::from_pubkey(*pubkey_bytes);
                        let msg = Message::from_hashed_data::<sha256::Hash>(&call.call_bytes);
                        (msg, addr)
                    }
                    SigType::Adr36 => {
                        let addr = Address::from_pubkey(*pubkey_bytes);
                        let bytes = adr36_bytes(call.call_bytes.as_slice(), addr)?;
                        let msg = Message::from_hashed_data::<sha256::Hash>(bytes.as_slice());
                        (msg, addr)
                    }
                    SigType::Sdk(tx) => {
                        let addr = Address::from_pubkey(*pubkey_bytes);
                        let bytes = self.sdk_sign_bytes(tx, addr)?;
                        let msg = Message::from_hashed_data::<sha256::Hash>(bytes.as_slice());
                        (msg, addr)
                    }
                    SigType::EthPersonalSign(tx) => {
                        let pubkey_bytes = pubkey.serialize_uncompressed();
                        let mut eth_pubkey = [0; 64];
                        eth_pubkey.copy_from_slice(&pubkey_bytes[1..]);
                        let addr = Address::from_pubkey_eth(eth_pubkey);

                        let prefix = b"\x19Ethereum Signed Message:\n";
                        let mut sdk_bytes = self.sdk_sign_bytes(tx, addr)?;
                        let mut len_bytes = sdk_bytes.len().to_string().as_bytes().to_vec();

                        let mut bytes = prefix.to_vec();
                        bytes.append(&mut len_bytes);
                        bytes.append(&mut sdk_bytes);

                        use sha3::{Digest, Keccak256};
                        let mut hasher = Keccak256::new();
                        hasher.update(&bytes);
                        let hash = hasher.finalize();

                        let msg = Message::from_slice(&hash)?;

                        (msg, addr)
                    }
                };

                let signature = Signature::from_compact(&signature)?;
                #[cfg(not(fuzzing))]
                secp.verify_ecdsa(&msg, &signature, &pubkey)?;

                Ok(Some(addr))
            }
            (None, None) => Ok(None),
            _ => Err(Error::Signer("Malformed transaction".into())),
        }
    }
}

impl<T: Call + State> Call for SignerPlugin<T>
where
    T: GetNonce,
{
    type Call = SignerCall;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        Context::remove::<Signer>();
        let signer_ctx = Signer {
            signer: self.verify(&call)?,
        };
        Context::add(signer_ctx);

        dbg!("decoding signer plugin inner call..");
        let inner_call = Decode::decode(call.call_bytes.as_slice())?;
        dbg!(&inner_call);
        self.inner.call(inner_call)
    }
}

impl<T: Query> Query for SignerPlugin<T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

pub(crate) fn sdk_to_signercall(sdk_tx: &SdkTx) -> Result<SignerCall> {
    let signature = sdk_tx.signature()?;
    let pubkey = sdk_tx.sender_pubkey()?;
    let sig_type = sdk_tx.sig_type()?;

    let sdk_tx = Box::new(sdk_tx.clone());
    let sigtype = match sig_type {
        None | Some("sdk") => SigType::Sdk(sdk_tx),
        Some("eth") => SigType::EthPersonalSign(sdk_tx),
        Some(_) => return Err(Error::App("Unknown signature type".to_string())),
    };

    Ok(SignerCall {
        signature: Some(signature),
        pubkey: Some(pubkey),
        sigtype,
        call_bytes: vec![],
    })
}

impl<T> ConvertSdkTx for SignerPlugin<T>
where
    T: State + ConvertSdkTx<Output = T::Call> + Call,
{
    type Output = SignerCall;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<SignerCall> {
        let mut signer_call = sdk_to_signercall(sdk_tx)?;
        signer_call.call_bytes = self.inner.convert(sdk_tx)?.encode()?;
        Ok(signer_call)
    }
}

pub struct SignerClient<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<fn() -> T>,
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "abci")]
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
            #[cfg(feature = "abci")]
            privkey: SecretKey::from_slice(&self.privkey.secret_bytes()).unwrap(),
            #[cfg(target_arch = "wasm32")]
            signer: keplr::Signer,
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
    #[cfg(not(feature = "abci"))]
    async fn call(&self, call: Self::Call) -> Result<()> {
        unimplemented!()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "abci")]
    async fn call(&self, call: Self::Call) -> Result<()> {
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
    async fn call(&self, call: Self::Call) -> Result<()> {
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
                sigtype: SigType::Adr36,
            })
            .await
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query,
        U: for<'a> AsyncQuery<Query = T::Query, Response<'a> = std::rc::Rc<SignerPlugin<T>>> + Clone,
    > AsyncQuery for SignerClient<T, U>
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

impl<T: Client<SignerClient<T, U>>, U: Clone> Client<U> for SignerPlugin<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(SignerClient {
            parent,
            marker: std::marker::PhantomData,
            #[cfg(not(target_arch = "wasm32"))]
            #[cfg(feature = "abci")]
            privkey: load_privkey().expect("Failed to load private key"),
            #[cfg(target_arch = "wasm32")]
            signer: keplr::Signer,
        })
    }
}

// impl<T> State for SignerPlugin<T>
// where
//     T: State,
// {
//     fn attach(&mut self, store: Store) -> Result<()> {
//         self.inner.attach(store)
//     }

//     fn flush<W: std::io::Write>(self, out: &mut W) -> Result<()> {
//         self.inner.flush(out)
//     }

//     fn load(store: Store, bytes: &mut &[u8]) -> Result<Self> {
//         let inner = T::load(store, bytes)?;
//         Ok(Self {
//             inner,
//             // ..Default::default()
//         })
//     }
// }

// impl<T> Describe for SignerPlugin<T>
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

#[cfg(target_arch = "wasm32")]
pub mod keplr {
    use super::Error;
    use crate::plugins::sdk_compat::sdk;
    use js_sys::{
        Array, Function, Object, Promise,
        Reflect::{apply, get},
        Uint8Array,
    };
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    pub struct Signer;

    pub struct KeplrHandle {
        keplr: Object,
        signer: JsValue,
        chain_id: String,
    }

    impl KeplrHandle {
        pub fn new() -> Self {
            unsafe {
                let window = web_sys::window().expect("no global `window` exists");
                let keplr = window.get("keplr").expect("no `keplr` in global `window`");

                let storage = window
                    .local_storage()
                    .expect("no `localStorage` in global `window`")
                    .expect("no `localStorage` in global `window`");
                let res = storage
                    .get("orga/chainid")
                    .expect("Could not load from local storage");
                let chain_id = match res {
                    Some(chain_id) => chain_id,
                    None => panic!("localStorage['orga/chainid'] is not set"),
                };

                let args = Array::new();

                Array::push(&args, &chain_id.clone().into());
                let get_offline_signer: Function =
                    get(&keplr, &"getOfflineSigner".to_string().into())
                        .unwrap()
                        .into();
                let signer = apply(&get_offline_signer, &keplr, &args).unwrap();

                Self {
                    keplr,
                    signer,
                    chain_id,
                }
            }
        }
    }

    impl Signer {
        fn handle(&self) -> KeplrHandle {
            KeplrHandle::new()
        }

        pub async fn pubkey(&self) -> [u8; 33] {
            unsafe {
                let signer = self.handle().signer;
                let get_accounts: Function = get(&signer, &"getAccounts".to_string().into())
                    .unwrap()
                    .into();
                let accounts_promise: Promise =
                    apply(&get_accounts, &signer, &Array::new()).unwrap().into();
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

        pub async fn address(&self) -> String {
            unsafe {
                let signer = self.handle().signer;
                let get_accounts: Function = get(&signer, &"getAccounts".to_string().into())
                    .unwrap()
                    .into();
                let accounts_promise: Promise =
                    apply(&get_accounts, &signer, &Array::new()).unwrap().into();
                let accounts = JsFuture::from(accounts_promise).await.unwrap();
                let account = get(&accounts, &0i32.into()).unwrap();
                get(&account, &"address".to_string().into())
                    .unwrap()
                    .as_string()
                    .unwrap()
            }
        }

        pub async fn sign(&self, call_bytes: &[u8]) -> [u8; 64] {
            unsafe {
                let msg = Array::new();
                for byte in call_bytes {
                    Array::push(&msg, &(*byte as i32).into());
                }

                let handle = self.handle();

                let args = Array::new();
                Array::push(&args, &handle.chain_id.clone().into());
                Array::push(&args, &self.address().await.into());
                Array::push(&args, &msg.into());

                let sign_arbitrary: Function =
                    get(&handle.keplr, &"signArbitrary".to_string().into())
                        .unwrap()
                        .into();
                let sign_promise: Promise =
                    apply(&sign_arbitrary, &handle.keplr, &args).unwrap().into();
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

        pub async fn sign_sdk(&self, sign_doc: sdk::SignDoc) -> Result<sdk::Signature, JsValue> {
            unsafe {
                let doc_json = serde_json::to_string(&sign_doc).unwrap();
                let doc_obj = js_sys::JSON::parse(&doc_json).unwrap();

                let args = Array::new();
                Array::push(&args, &sign_doc.chain_id.clone().into());
                Array::push(&args, &self.address().await.into());
                Array::push(&args, &doc_obj);

                let handle = self.handle();

                let sign_amino: Function =
                    get(&handle.keplr, &"signAmino".to_string().into())?.into();
                let sign_promise: Promise =
                    apply(&sign_amino, &handle.keplr, &args).unwrap().into();
                let res = JsFuture::from(sign_promise).await.unwrap();

                let signature = get(&res, &"signature".to_string().into()).unwrap();
                let signature_json: String = js_sys::JSON::stringify(&signature).unwrap().into();
                Ok(serde_json::from_str(&signature_json).unwrap())
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "abci")]
pub fn load_privkey() -> Result<SecretKey> {
    // Ensure orga home directory exists
    let orga_home = home::home_dir()
        .expect("No home directory set")
        .join(".orga-wallet");

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
        std::fs::write(&keypair_path, privkey.secret_bytes())?;
        Ok(privkey)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct KeyPair {
    pub private: SecretKey,
    pub public: PublicKey,
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "abci")]
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

    impl<T> crate::abci::AbciQuery for SignerPlugin<T>
    where
        T: crate::abci::AbciQuery + State + Call,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::abci::RequestQuery,
        ) -> Result<tendermint_proto::abci::ResponseQuery> {
            self.inner.abci_query(request)
        }
    }
}

#[orga]
struct Counter {
    pub count: u64,
    pub last_signer: Address,
}

#[orga]
impl Counter {
    #[call]
    pub fn increment(&mut self) -> Result<()> {
        self.count += 1;
        let signer = self.context::<Signer>().unwrap().signer.unwrap();
        self.last_signer = signer;

        Ok(())
    }
}

impl GetNonce for Counter {
    fn nonce(&self, _address: Address) -> Result<u64> {
        Ok(0)
    }
}

#[derive(State, Clone, Debug, Encode, Decode, Default, MigrateFrom)]
pub struct X(());
impl Symbol for X {
    const INDEX: u8 = 99;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::Call;
    use crate::plugins::{sdk_compat, ConvertSdkTx, SdkCompatPlugin};

    impl ConvertSdkTx for Counter {
        type Output = <Counter as Call>::Call;

        fn convert(&self, _msg: &sdk_compat::sdk::Tx) -> Result<Self::Output> {
            Ok(<Counter as Call>::Call::Method(
                CounterMethodCall::Increment(),
            ))
        }
    }

    #[test]
    fn eth_personal_sign() {
        let mut state = SdkCompatPlugin {
            symbol: std::marker::PhantomData::<X>,
            inner: SignerPlugin {
                inner: Counter {
                    count: 0,
                    last_signer: Address::NULL,
                },
            },
        };

        Context::add(ChainId("testchain"));

        // sign bytes: {"account_number":"0","chain_id":"testchain","fee":{"amount":[{"amount":"0","denom":"unom"}],"gas":"10000"},"memo":"","msgs":[{"type":"x","value":{}}],"sequence":"1"}
        // signature and pubkey taken from metamask
        let call_bytes = br#"{"msg":[{"type":"x","value":{}}],"fee":{"amount":[{"amount":"0","denom":"unom"}],"gas":"10000"},"memo":"","signatures":[{"pub_key":{"type":"tendermint/PubKeySecp256k1","value":"AgixpAV7cl5HPnmZC5qmJekVd5E8VZUioqrJoaj36p90"},"signature":"w+ZKyFdmhDOoqLIlhZq+yj8Z+eMOZnyjYKQ5rXr/fS4Imt4n5rTbwgHR1TmF6mGdFvZrmeJFedUjyMjnRYV4bA==","type":"eth"}]}"#;
        let call = Decode::decode(call_bytes.as_slice()).unwrap();
        SdkCompatPlugin::<_, _>::call(&mut state, call).unwrap();

        assert_eq!(state.inner.inner.count, 1);
        assert_eq!(
            state.inner.inner.last_signer,
            [
                147, 54, 126, 195, 164, 236, 108, 70, 107, 218, 16, 43, 121, 200, 38, 174, 234,
                199, 157, 75
            ]
            .into()
        );
        Context::remove::<ChainId>();
    }
}
