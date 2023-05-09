use crate::{
    coins::Address,
    plugins::{SigType, SignerCall},
    Result,
};

pub trait Wallet: Clone {
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall>;

    fn address(&self) -> Result<Option<Address>>;

    fn nonce_hint(&self) -> Result<Option<u64>>;
}

#[derive(Clone, Default)]
pub struct Unsigned;

impl Wallet for Unsigned {
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall> {
        Ok(SignerCall {
            call_bytes: call_bytes.to_vec(),
            signature: None,
            pubkey: None,
            sigtype: SigType::Native,
        })
    }

    fn address(&self) -> Result<Option<Address>> {
        Ok(None)
    }

    fn nonce_hint(&self) -> Result<Option<u64>> {
        Ok(None)
    }
}

// TODO: implement file wallet
