use super::Context;
use crate::call::Call;
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::Result;
use tendermint::public_key::Ed25519;
use tendermint::signature::Ed25519Signature;

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
                let pubkey = Ed25519::from_bytes(&pubkey_bytes)?;
                let call_bytes = Encode::encode(&self.call_bytes)?;
                let signature = Ed25519Signature::new(signature);
                pubkey.verify_strict(&call_bytes, &signature)?;

                Ok(Some(pubkey_bytes))
            }
            (None, None) => Ok(None),
            _ => failure::bail!("Malformed transaction"),
        }
    }
}

impl<T: Call + State> Call for SignerProvider<T> {
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

impl<T> State for SignerProvider<T>
where
    T: State,
    T::Encoding: From<T>,
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
    T::Encoding: From<T>,
{
    fn from(provider: SignerProvider<T>) -> Self {
        (provider.inner.into(),)
    }
}
