use super::{BeginBlockCtx, EndBlockCtx, GetContext, InitChainCtx, Signer};
use crate::abci::{BeginBlock, EndBlock, InitChain};
use crate::call::Call;
use crate::collections::Map;
use crate::encoding::{Decode, Encode};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::Result;

type NonceMap = Map<[u8; 32], u64>;

pub struct NonceProvider<T> {
    inner: T,
    map: NonceMap,
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
                if nonce != *expected_nonce {
                    failure::bail!("Nonce is not valid.");
                }
                *expected_nonce += 1;
                self.inner.call(call.inner_call)
            }
            (None, None) => self.inner.call(call.inner_call),

            // Unhappy paths:
            (Some(_), None) => failure::bail!("Signed calls must include a nonce."),
            (None, Some(_)) => failure::bail!("Unsigned calls must not include a nonce."),
        }
    }
}

impl<T: Query> Query for NonceProvider<T> {
    type Query = T::Query;

    fn query(&self, query: Self::Query) -> Result<()> {
        self.inner.query(query)
    }
}

impl<T> State for NonceProvider<T>
where
    T: State,
{
    type Encoding = (<NonceMap as State>::Encoding, T::Encoding);

    fn create(store: Store, data: Self::Encoding) -> Result<Self> {
        Ok(Self {
            map: NonceMap::create(store.sub(&[0]), data.0)?,
            inner: T::create(store.sub(&[1]), data.1)?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok((self.map.flush()?, self.inner.flush()?))
    }
}

impl<T> From<NonceProvider<T>> for (<NonceMap as State>::Encoding, T::Encoding)
where
    T: State,
{
    fn from(provider: NonceProvider<T>) -> Self {
        (provider.map.into(), provider.inner.into())
    }
}

// TODO: Remove dependency on ABCI for this otherwise-pure plugin.

impl<T> BeginBlock for NonceProvider<T>
where
    T: BeginBlock + State,
{
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.inner.begin_block(ctx)
    }
}

impl<T> EndBlock for NonceProvider<T>
where
    T: EndBlock + State,
{
    fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
        self.inner.end_block(ctx)
    }
}

impl<T> InitChain for NonceProvider<T>
where
    T: InitChain + State,
{
    fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
        self.inner.init_chain(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Signer;
    use super::*;
    use crate::contexts::Context;
    use crate::store::{MapStore, Shared, Store};

    #[derive(State)]
    struct CounterApp {
        pub count: u64,
    }

    impl CounterApp {
        fn increment(&mut self) -> Result<()> {
            self.count += 1;

            Ok(())
        }
    }

    impl Call for CounterApp {
        type Call = ();

        fn call(&mut self, _call: Self::Call) -> Result<()> {
            self.increment()
        }
    }

    fn nonced_call(n: u64) -> NonceCall<()> {
        NonceCall {
            nonce: Some(n),
            inner_call: (),
        }
    }

    fn unnonced_call() -> NonceCall<()> {
        NonceCall {
            nonce: None,
            inner_call: (),
        }
    }

    #[test]
    fn nonced_calls() {
        let store = Shared::new(MapStore::new());
        let mut state =
            NonceProvider::<CounterApp>::create(Store::new(store.into()), Default::default())
                .unwrap();

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
            signer: Some([0; 32]),
        });

        // Signed, correct nonce
        state.call(nonced_call(0)).unwrap();
        assert_eq!(state.inner.count, 2);

        // Signed, incorrect nonces
        assert!(state.call(nonced_call(0)).is_err());
        assert!(state.call(nonced_call(123)).is_err());

        // Signed, no nonce
        assert!(state.call(unnonced_call()).is_err());
        Context::remove::<Signer>();
    }
}
