use super::{BeginBlockCtx, Context, EndBlockCtx, InitChainCtx};
use crate::abci::{BeginBlock, EndBlock, InitChain};
use crate::call::Call;
use crate::client::Client;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer as Ed25519Signer};

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
                let pubkey = PublicKey::from_bytes(&pubkey_bytes)?;
                let signature = Signature::new(signature);
                pubkey.verify_strict(&self.call_bytes, &signature)?;

                Ok(Some(pubkey_bytes))
            }
            (None, None) => Ok(None),
            _ => {
                return Err(Error::Signer("Malformed transaction".into()));
            }
        }
    }
}

impl<T: Call> Call for SignerProvider<T> {
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

pub struct SignerClient<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<T>,
    keypair: Keypair,
}

impl<T, U: Clone> Clone for SignerClient<T, U> {
    fn clone(&self) -> Self {
        SignerClient {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
            keypair: Keypair::from_bytes(&self.keypair.to_bytes()).unwrap(),
        }
    }
}

impl<T: Call, U: Call<Call = SignerCall> + Clone> Call for SignerClient<T, U> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let call_bytes = Encode::encode(&call)?;
        let signature = self.keypair.sign(call_bytes.as_slice()).to_bytes();
        let pubkey = self.keypair.public.to_bytes();

        self.parent.call(SignerCall {
            call_bytes,
            pubkey: Some(pubkey),
            signature: Some(signature),
        })
    }
}

impl<T: Client<SignerClient<T, U>>, U: Clone> Client<U> for SignerProvider<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(SignerClient {
            parent,
            marker: std::marker::PhantomData,
            keypair: load_keypair().expect("Failed to load keypair"),
        })
    }
}

impl<T> State for SignerProvider<T>
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

impl<T> From<SignerProvider<T>> for (T::Encoding,)
where
    T: State,
{
    fn from(provider: SignerProvider<T>) -> Self {
        (provider.inner.into(),)
    }
}

fn load_keypair() -> Result<Keypair> {
    use rand_core::OsRng;
    // Ensure orga home directory exists

    let orga_home = home::home_dir()
        .expect("No home directory set")
        .join(".orga");

    std::fs::create_dir_all(&orga_home)?;
    let keypair_path = orga_home.join("privkey");
    if keypair_path.exists() {
        // Load existing key
        let bytes = std::fs::read(&keypair_path)?;
        Ok(Keypair::from_bytes(bytes.as_slice())?)
    } else {
        // Create and save a new key
        let mut csprng = OsRng {};
        let keypair = Keypair::generate(&mut csprng);
        std::fs::write(&keypair_path, keypair.to_bytes())?;
        Ok(keypair)
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::call::Call;
    use crate::contexts::GetContext;
    use crate::state::State;

    #[derive(State, Clone)]
    struct Counter {
        pub count: u64,
        pub last_signer: Option<[u8; 32]>,
    }

    impl Counter {
        fn increment(&mut self) -> Result<()> {
            self.count += 1;
            let signer = self.context::<Signer>().unwrap().signer.unwrap();
            self.last_signer.replace(signer);

            Ok(())
        }
    }

    #[derive(Encode, Decode)]
    pub enum CounterCall {
        Increment,
    }

    impl Call for Counter {
        type Call = CounterCall;

        fn call(&mut self, call: Self::Call) -> Result<()> {
            match call {
                CounterCall::Increment => self.increment(),
            }
        }
    }

    #[derive(Clone)]
    struct CounterClient<T> {
        parent: T,
    }

    impl<T: Call<Call = CounterCall> + Clone> CounterClient<T> {
        pub fn increment(&mut self) -> Result<()> {
            self.parent.call(CounterCall::Increment)
        }
    }

    impl<T: Clone> Client<T> for Counter {
        type Client = CounterClient<T>;

        fn create_client(parent: T) -> Self::Client {
            CounterClient { parent }
        }
    }

    #[test]
    fn signed_increment() {
        let state = Rc::new(RefCell::new(SignerProvider {
            inner: Counter {
                count: 0,
                last_signer: None,
            },
        }));
        let mut client = SignerProvider::<Counter>::create_client(state.clone());
        client.increment().unwrap();
        assert_eq!(state.borrow().inner.count, 1);
        let pub_key = load_keypair().unwrap().public.to_bytes();
        assert_eq!(state.borrow().inner.last_signer, Some(pub_key));
    }
}
