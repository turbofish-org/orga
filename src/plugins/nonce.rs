//! Incrementing nonces per address for calls.
use orga_macros::orga;

use super::{sdk_compat::sdk::Tx as SdkTx, ConvertSdkTx, Signer};
use crate::call::Call;
use crate::coins::Address;
use crate::collections::Map;
use crate::context::GetContext;

use crate::encoding::{Decode, Encode};

use crate::state::State;
use crate::{Error, Result};

const NONCE_INCREASE_LIMIT: u64 = 1000;

/// A plugin which requires calls to be issued with a valid nonce, incrementing
/// for each address each call.
///
/// Calls must include a nonce (`u64`) which is greater than the last one stored
/// for that address, by no more than 1000.
///
/// Nonces may be queried by clients before issuing calls.
#[orga(skip(Call))]
pub struct NoncePlugin<T> {
    /// Stored nonces for each address. Implicitly 0 for addresses without a
    /// stored value.
    pub map: Map<Address, u64>,
    /// The inner value.
    pub inner: T,
}

impl<T: State> NoncePlugin<T> {
    /// Returns the nonce for the given address, or 0 if the address has no
    /// stored nonce.
    pub fn nonce(&self, address: Address) -> Result<u64> {
        Ok(*self.map.get_or_default(address)?)
    }
}

/// A trait for types which can determine the nonce for an address.
pub trait GetNonce {
    /// Returns the nonce for the given address.
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

/// A call which may also include a nonce.
#[derive(Debug, Encode, Decode)]
pub struct NonceCall<T> {
    /// Optional nonce.
    pub nonce: Option<u64>,
    /// The inner call.
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

    #[serial_test::serial]
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
