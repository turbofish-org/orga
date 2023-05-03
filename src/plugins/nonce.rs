use serde::Serialize;

use super::{sdk_compat::sdk::Tx as SdkTx, ConvertSdkTx, Signer};
use crate::call::Call;
use crate::client::Client;
use crate::client::{AsyncCall, AsyncQuery};
use crate::coins::Address;
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::migrate::{MigrateFrom, MigrateInto};
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};
use std::ops::{Deref, DerefMut};

const NONCE_INCREASE_LIMIT: u64 = 1000;

#[derive(State, Encode, Decode, Default, Serialize)]
pub struct NoncePlugin<T: State> {
    map: Map<Address, u64>,
    inner: T,
}

impl<T1: State, T2: State> MigrateFrom<NoncePlugin<T1>> for NoncePlugin<T2>
where
    T1: MigrateInto<T2>,
{
    fn migrate_from(other: NoncePlugin<T1>) -> Result<Self> {
        Ok(Self {
            map: other.map.migrate_into()?,
            inner: other.inner.migrate_into()?,
        })
    }
}

impl<T: State> NoncePlugin<T> {
    pub fn nonce(&self, address: Address) -> Result<u64> {
        Ok(*self.map.get_or_default(address)?)
    }
}

impl<T: State> Deref for NoncePlugin<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: State> DerefMut for NoncePlugin<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub trait GetNonce {
    fn nonce(&self, address: Address) -> Result<u64>;
}

impl<T: State> GetNonce for NoncePlugin<T> {
    fn nonce(&self, address: Address) -> Result<u64> {
        self.nonce(address)
    }
}

impl<T> ConvertSdkTx for NoncePlugin<T>
where
    T: State + ConvertSdkTx<Output = T::Call> + Call,
{
    type Output = NonceCall<T::Call>;

    fn convert(&self, sdk_tx: &SdkTx) -> Result<NonceCall<T::Call>> {
        let address = sdk_tx.sender_address()?;
        let nonce = self.nonce(address)? + 1;
        let inner_call = self.inner.convert(sdk_tx)?;

        Ok(NonceCall {
            inner_call,
            nonce: Some(nonce),
        })
    }
}

#[derive(Encode, Decode, Debug)]
pub enum NonceQuery<T> {
    Nonce(Address),
    Inner(T),
}

impl<T: State + Query> Query for NoncePlugin<T> {
    type Query = NonceQuery<T::Query>;

    fn query(&self, query: Self::Query) -> Result<()> {
        match query {
            NonceQuery::Nonce(address) => {
                self.nonce(address)?;
                Ok(())
            }
            NonceQuery::Inner(query) => self.inner.query(query),
        }
    }
}

#[derive(Debug, Encode, Decode)]
pub struct NonceCall<T> {
    pub nonce: Option<u64>,
    pub inner_call: T,
}

impl<T> Call for NoncePlugin<T>
where
    T: Call + State,
{
    type Call = NonceCall<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let signer = match self.context::<Signer>() {
            Some(signer) => signer,
            None => {
                return Err(Error::Nonce(
                    "Nonce could not resolve the Signer context".into(),
                ));
            }
        };

        match (signer.signer, call.nonce) {
            // Happy paths:
            (Some(pub_key), Some(nonce)) => {
                let mut expected_nonce = self.map.entry(pub_key)?.or_default()?;
                if nonce <= *expected_nonce {
                    return Err(Error::Nonce(format!(
                        "Nonce is not valid. Expected {}-{}, got {}",
                        *expected_nonce + 1,
                        *expected_nonce + NONCE_INCREASE_LIMIT,
                        nonce,
                    )));
                }

                if nonce - *expected_nonce > NONCE_INCREASE_LIMIT {
                    return Err(Error::Nonce(format!(
                        "Nonce increase is too large: {}",
                        nonce - *expected_nonce
                    )));
                }

                *expected_nonce = nonce;
                self.inner.call(call.inner_call)
            }
            (None, None) => self.inner.call(call.inner_call),

            // Unhappy paths:
            (Some(_), None) => Err(Error::Nonce("Signed calls must include a nonce".into())),
            (None, Some(_)) => Err(Error::Nonce(
                "Unsigned calls must not include a nonce".into(),
            )),
        }
    }
}

pub struct NonceAdapter<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<fn() -> T>,
}

unsafe impl<T, U: Send + Clone> Send for NonceAdapter<T, U> {}

impl<T, U: Clone> Clone for NonceAdapter<T, U> {
    fn clone(&self) -> Self {
        NonceAdapter {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Call + Query + State, U> AsyncCall for NonceAdapter<T, U>
where
    T::Call: Send,
    U: Send
        + AsyncCall<Call = NonceCall<T::Call>>
        + Clone
        + for<'a> AsyncQuery<Query = NonceQuery<T::Query>, Response<'a> = std::rc::Rc<NoncePlugin<T>>>,
{
    type Call = T::Call;

    async fn call(&self, call: Self::Call) -> Result<()> {
        // Load nonce from file
        let nonce = load_nonce()?;

        let call_bytes = call.encode()?;
        let fut = self.parent.call(NonceCall {
            inner_call: call,
            nonce: Some(nonce),
        });

        // Increment the local nonce
        write_nonce(nonce + 1)?;

        let res = fut.await;

        // TODO: ABCI shouldn't be messing up the error variant
        // if let Err(Error::Nonce(_)) = res {
        #[cfg(not(target_arch = "wasm32"))]
        #[cfg(feature = "abci")]
        if let Err(err) = &res {
            if err.to_string().contains("Nonce Error:") {
                let privkey = super::signer::load_privkey().unwrap();
                let pubkey =
                    secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &privkey);
                let my_addr = Address::from_pubkey(pubkey.serialize());
                let nonce = self
                    .parent
                    .query(NonceQuery::Nonce(my_addr), |n| n.nonce(my_addr))
                    .await?;

                let call = Self::Call::decode(call_bytes.as_slice())?;
                let fut = self.parent.call(NonceCall {
                    inner_call: call,
                    nonce: Some(nonce + 1),
                });

                // Increment the local nonce
                write_nonce(nonce + 2)?;

                return fut.await;
            }
        }

        res
    }
}

#[async_trait::async_trait(?Send)]
impl<
        T: Query + State,
        U: for<'a> AsyncQuery<
                Query = NonceQuery<T::Query>,
                Response<'a> = std::rc::Rc<NoncePlugin<T>>,
            > + Clone,
    > AsyncQuery for NonceAdapter<T, U>
{
    type Query = T::Query;
    type Response<'a> = std::rc::Rc<T>;

    async fn query<F, R>(&self, query: Self::Query, mut check: F) -> Result<R>
    where
        F: FnMut(Self::Response<'_>) -> Result<R>,
    {
        self.parent
            .query(NonceQuery::Inner(query), |plugin| {
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

pub struct NonceClient<T: Client<NonceAdapter<T, U>> + State, U: Clone> {
    inner: T::Client,
    parent: U,
}

impl<T: Client<NonceAdapter<T, U>> + State, U: Clone> Clone for NonceClient<T, U>
where
    T::Client: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            parent: self.parent.clone(),
        }
    }
}

impl<T: Client<NonceAdapter<T, U>> + State, U: Clone> Deref for NonceClient<T, U> {
    type Target = T::Client;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Client<NonceAdapter<T, U>> + State, U: Clone> DerefMut for NonceClient<T, U> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<
        T: Client<NonceAdapter<T, U>> + State + Query,
        U: Clone
            + for<'a> AsyncQuery<
                Query = NonceQuery<T::Query>,
                Response<'a> = std::rc::Rc<NoncePlugin<T>>,
            >,
    > NonceClient<T, U>
{
    pub async fn nonce(&self, address: Address) -> Result<u64> {
        self.parent
            .query(NonceQuery::Nonce(address), |plugin| plugin.nonce(address))
            .await
    }
}

impl<T: Client<NonceAdapter<T, U>> + State, U: Clone> Client<U> for NoncePlugin<T> {
    type Client = NonceClient<T, U>;

    fn create_client(parent: U) -> Self::Client {
        NonceClient {
            inner: T::create_client(NonceAdapter {
                parent: parent.clone(),
                marker: std::marker::PhantomData,
            }),
            parent,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "abci")]
fn nonce_path() -> Result<std::path::PathBuf> {
    let orga_home = home::home_dir()
        .expect("No home directory set")
        .join(".orga-wallet");

    std::fs::create_dir_all(&orga_home)?;
    Ok(orga_home.join("nonce"))
}

#[cfg(target_arch = "wasm32")]
fn load_nonce() -> Result<u64> {
    let window = web_sys::window().unwrap();
    let storage = window
        .local_storage()
        .map_err(|_| Error::Nonce("Could not get local storage".into()))?
        .unwrap();
    let res = storage
        .get("orga/nonce")
        .map_err(|_| Error::Nonce("Could not load from local storage".into()))?;
    match res {
        Some(nonce) => Ok(nonce.parse()?),
        None => Ok(1),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "abci")]
fn load_nonce() -> Result<u64> {
    let nonce_path = nonce_path()?;
    if nonce_path.exists() {
        let bytes = std::fs::read(&nonce_path)?;
        Ok(Decode::decode(bytes.as_slice())?)
    } else {
        let bytes = 1u64.encode()?;
        std::fs::write(&nonce_path, bytes)?;
        Ok(1)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(feature = "abci"))]
fn load_nonce() -> Result<u64> {
    unimplemented!()
}

#[cfg(target_arch = "wasm32")]
fn write_nonce(nonce: u64) -> Result<()> {
    let window = web_sys::window().unwrap();
    let storage = window
        .local_storage()
        .map_err(|_| Error::Nonce("Could not get local storage".into()))?
        .unwrap();
    storage
        .set("orga/nonce", nonce.to_string().as_str())
        .map_err(|_| Error::Nonce("Could not write to local storage".into()))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "abci")]
fn write_nonce(nonce: u64) -> Result<()> {
    let nonce_path = nonce_path()?;
    Ok(std::fs::write(nonce_path, nonce.encode()?)?)
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(feature = "abci"))]
fn write_nonce(nonce: u64) -> Result<()> {
    unimplemented!()
}

// TODO: Remove dependency on ABCI for this otherwise-pure plugin.
#[cfg(feature = "abci")]
mod abci {
    use super::super::{BeginBlockCtx, EndBlockCtx, InitChainCtx};
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<T> BeginBlock for NoncePlugin<T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<T> EndBlock for NoncePlugin<T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<T> InitChain for NoncePlugin<T>
    where
        T: InitChain + State + Call,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }

    impl<T> crate::abci::AbciQuery for NoncePlugin<T>
    where
        T: crate::abci::AbciQuery + State + Call,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::v0_34::abci::RequestQuery,
        ) -> Result<tendermint_proto::v0_34::abci::ResponseQuery> {
            self.inner.abci_query(request)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Signer;
    use super::*;
    use crate::context::Context;

    #[derive(State, Encode, Decode, Default)]
    struct Counter {
        pub count: u64,
    }

    impl Counter {
        fn increment(&mut self) -> Result<()> {
            self.count += 1;

            Ok(())
        }
    }

    #[derive(Debug, Encode, Decode)]
    enum CounterCall {
        Increment,
    }

    impl Call for Counter {
        type Call = CounterCall;

        fn call(&mut self, _call: Self::Call) -> Result<()> {
            self.increment()
        }
    }

    fn nonced_call(n: u64) -> NonceCall<CounterCall> {
        NonceCall {
            nonce: Some(n),
            inner_call: CounterCall::Increment,
        }
    }

    fn unnonced_call() -> NonceCall<CounterCall> {
        NonceCall {
            nonce: None,
            inner_call: CounterCall::Increment,
        }
    }

    #[test]
    fn nonced_calls() {
        let mut state: NoncePlugin<Counter> = Default::default();

        // Fails if the signer context isn't available.
        assert!(state.call(unnonced_call()).is_err());

        Context::add(Signer { signer: None });
        // No signature, no nonce
        state.call(unnonced_call()).unwrap();

        // No signature, with nonce
        assert!(state.call(nonced_call(0)).is_err());

        assert_eq!(state.inner.count, 1);
        Context::remove::<Signer>();
        Context::add(Signer {
            signer: Some(Address::from_pubkey([0; 33])),
        });

        // Signed, correct nonce
        state.call(nonced_call(1)).unwrap();
        assert_eq!(state.inner.count, 2);

        // Signed, but nonce incremented by too much
        assert!(state.call(nonced_call(2000)).is_err());

        // Signed, incorrect nonce
        assert!(state.call(nonced_call(0)).is_err());

        // Signed, no nonce
        assert!(state.call(unnonced_call()).is_err());
        Context::remove::<Signer>();
    }
}
