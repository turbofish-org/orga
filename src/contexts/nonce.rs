use super::{GetContext, Signer};
use crate::call::Call;
use crate::client::{AsyncCall, Client};
use crate::coins::Address;
use crate::collections::Map;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::Result;
use std::ops::Deref;
use std::path::PathBuf;

#[derive(State, Encode, Decode)]
pub struct NonceProvider<T: State> {
    map: Map<Address, u64>,
    inner: T,
}

impl<T: State> Deref for NonceProvider<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Encode, Decode)]
pub struct NonceCall<T> {
    nonce: Option<u64>,
    inner_call: T,
}

impl<T> Call for NonceProvider<T>
where
    T: Call + State,
{
    type Call = NonceCall<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let signer = match self.context::<Signer>() {
            Some(signer) => signer,
            None => failure::bail!("Nonce could not resolve the Signer context."),
        };

        match (signer.signer, call.nonce) {
            // Happy paths:
            (Some(pub_key), Some(nonce)) => {
                let mut expected_nonce = self.map.entry(pub_key)?.or_default()?;
                if nonce < *expected_nonce {
                    failure::bail!("Nonce is not valid.");
                }
                *expected_nonce = nonce + 1;
                self.inner.call(call.inner_call)
            }
            (None, None) => self.inner.call(call.inner_call),

            // Unhappy paths:
            (Some(_), None) => failure::bail!("Signed calls must include a nonce."),
            (None, Some(_)) => failure::bail!("Unsigned calls must not include a nonce."),
        }
    }
}

impl<T: Query + State> Query for NonceProvider<T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

pub struct NonceClient<T, U: Clone> {
    parent: U,
    marker: std::marker::PhantomData<T>,
}

unsafe impl<T, U: Send + Clone> Send for NonceClient<T, U> {}

impl<T, U: Clone> Clone for NonceClient<T, U> {
    fn clone(&self) -> Self {
        NonceClient {
            parent: self.parent.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<T: Call, U: AsyncCall<Call = NonceCall<T::Call>> + Clone> AsyncCall for NonceClient<T, U>
where
    T::Call: Send,
    U: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        // Load nonce from file
        let nonce = load_nonce()?;

        let res = self.parent.call(NonceCall {
            inner_call: call,
            nonce: Some(nonce),
        });

        // Increment the local nonce
        write_nonce(nonce + 1)?;

        res.await
    }
}

impl<T: Client<NonceClient<T, U>> + State, U: Clone> Client<U> for NonceProvider<T> {
    type Client = T::Client;

    fn create_client(parent: U) -> Self::Client {
        T::create_client(NonceClient {
            parent,
            marker: std::marker::PhantomData,
        })
    }
}

// impl<T> State for NonceProvider<T>
// where
//     T: State,
// {
//     type Encoding = (<NonceMap as State>::Encoding, T::Encoding);

//     fn create(store: Store, data: Self::Encoding) -> Result<Self> {
//         Ok(Self {
//             map: NonceMap::create(store.sub(&[0]), data.0)?,
//             inner: T::create(store.sub(&[1]), data.1)?,
//         })
//     }

//     fn flush(self) -> Result<Self::Encoding> {
//         Ok((self.map.flush()?, self.inner.flush()?))
//     }
// }

fn nonce_path() -> Result<PathBuf> {
    let orga_home = home::home_dir()
        .expect("No home directory set")
        .join(".orga");

    std::fs::create_dir_all(&orga_home)?;
    Ok(orga_home.join("nonce"))
}
fn load_nonce() -> Result<u64> {
    let nonce_path = nonce_path()?;
    if nonce_path.exists() {
        let bytes = std::fs::read(&nonce_path)?;
        Ok(Decode::decode(bytes.as_slice())?)
    } else {
        let bytes = 0.encode()?;
        std::fs::write(&nonce_path, bytes)?;
        Ok(0)
    }
}

fn write_nonce(nonce: u64) -> Result<()> {
    let nonce_path = nonce_path()?;
    Ok(std::fs::write(&nonce_path, nonce.encode()?)?)
}

// impl<T> From<NonceProvider<T>> for (<NonceMap as State>::Encoding, T::Encoding)
// where
//     T: State,
// {
//     fn from(provider: NonceProvider<T>) -> Self {
//         (provider.map.into(), provider.inner.into())
//     }
// }

// TODO: Remove dependency on ABCI for this otherwise-pure plugin.

// impl<T> BeginBlock for NonceProvider<T>
// where
//     T: BeginBlock + State,
// {
//     fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
//         self.inner.begin_block(ctx)
//     }
// }

// impl<T> EndBlock for NonceProvider<T>
// where
//     T: EndBlock + State,
// {
//     fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
//         self.inner.end_block(ctx)
//     }
// }

// impl<T> InitChain for NonceProvider<T>
// where
//     T: InitChain + State + Call,
// {
//     fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
//         self.inner.init_chain(ctx)
//     }
// }

#[cfg(test)]
mod tests {
    use super::super::Signer;
    use super::*;
    use crate::contexts::Context;
    use crate::store::{MapStore, Shared, Store};

    #[derive(State)]
    struct Counter {
        pub count: u64,
    }

    impl Counter {
        fn increment(&mut self) -> Result<()> {
            self.count += 1;

            Ok(())
        }
    }

    #[derive(Encode, Decode)]
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
        let store = Shared::new(MapStore::new());
        let mut state =
            NonceProvider::<Counter>::create(Store::new(store.into()), Default::default()).unwrap();

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
            signer: Some([0; 32].into()),
        });

        // Signed, correct nonce
        state.call(nonced_call(0)).unwrap();
        assert_eq!(state.inner.count, 2);

        // Signed, incorrect nonce
        assert!(state.call(nonced_call(0)).is_err());

        // Signed, no nonce
        assert!(state.call(unnonced_call()).is_err());
        Context::remove::<Signer>();
    }
}
