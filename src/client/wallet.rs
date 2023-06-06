use crate::{
    coins::Address,
    plugins::{SigType, SignerCall},
    Result,
};

pub trait Wallet: Clone {
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall>;

    fn address(&self) -> Result<Option<Address>>;

    fn nonce_hint(&self) -> Result<Option<u64>> {
        Ok(None)
    }
}

#[derive(Clone, Debug, Default)]
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
}

/// A wallet that derives a private key from a seed - intended to be used in
/// tests.
#[derive(Clone, Debug)]
pub struct DerivedKey {
    privkey: secp256k1::SecretKey,
}

impl DerivedKey {
    pub fn new(seed: &[u8]) -> Result<Self> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(seed);
        let hash = hasher.finalize();

        let privkey = secp256k1::SecretKey::from_slice(&hash)?;

        Ok(Self { privkey })
    }

    pub fn address_for(seed: &[u8]) -> Result<Address> {
        Ok(Self::new(seed)?.address())
    }

    pub fn privkey(&self) -> &secp256k1::SecretKey {
        &self.privkey
    }

    pub fn pubkey(&self) -> secp256k1::PublicKey {
        let secp = secp256k1::Secp256k1::new();
        secp256k1::PublicKey::from_secret_key(&secp, &self.privkey)
    }

    pub fn address(&self) -> Address {
        Address::from_pubkey(self.pubkey().serialize())
    }
}

impl Wallet for DerivedKey {
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall> {
        use secp256k1::hashes::sha256;
        let secp = secp256k1::Secp256k1::new();
        let msg = secp256k1::Message::from_hashed_data::<sha256::Hash>(call_bytes);
        let sig = secp.sign_ecdsa(&msg, &self.privkey);
        let sig_bytes = sig.serialize_compact();

        Ok(SignerCall {
            call_bytes: call_bytes.to_vec(),
            signature: Some(sig_bytes),
            pubkey: Some(self.pubkey().serialize()),
            sigtype: SigType::Native,
        })
    }

    fn address(&self) -> Result<Option<Address>> {
        Ok(Some(self.address()))
    }
}

// TODO: implement file wallet
