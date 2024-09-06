//! Key management for clients.
use std::path::Path;

use secp256k1::SecretKey;

use crate::{
    coins::Address,
    plugins::{SigType, SignerCall},
    Result,
};

/// A trait for wallets which can manage user keys.
pub trait Wallet: Clone + Send + Sync {
    /// Sign a call.
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall>;

    /// Returns the address for this wallet if it has one.
    fn address(&self) -> Result<Option<Address>>;

    /// Optionally returns a suggested nonce, e.g. by querying the [NoncePlugin]
    /// or using a local incrementing counter.
    fn nonce_hint(&self) -> Result<Option<u64>> {
        Ok(None)
    }
}

/// A wallet without keys. It produces unsigned calls and has no address.
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
    /// Derive a new key from a seed.
    pub fn new(seed: &[u8]) -> Result<Self> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(seed);
        let hash = hasher.finalize();

        let privkey = secp256k1::SecretKey::from_slice(&hash)?;

        Ok(Self { privkey })
    }

    /// Create a new wallet from a secret key.
    pub fn from_secret_key(privkey: SecretKey) -> Self {
        Self { privkey }
    }

    /// Create a new wallet from a seed and return its address.
    pub fn address_for(seed: &[u8]) -> Result<Address> {
        Ok(Self::new(seed)?.address())
    }

    /// Returns a reference to the secret key for this wallet.
    pub fn privkey(&self) -> &secp256k1::SecretKey {
        &self.privkey
    }

    /// Returns the public key for this wallet.
    pub fn pubkey(&self) -> secp256k1::PublicKey {
        let secp = secp256k1::Secp256k1::new();
        secp256k1::PublicKey::from_secret_key(&secp, &self.privkey)
    }

    /// Returns the address for this wallet.
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

/// A simple wallet with a secp256k1 keypair.
#[derive(Clone, Debug)]
pub struct SimpleWallet {
    privkey: secp256k1::SecretKey,
    pubkey: secp256k1::PublicKey,
}

impl SimpleWallet {
    /// Load the keypair from the specified path, creating it if it doesn't
    /// exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        std::fs::create_dir_all(&path)?;
        let keypair_path = path.as_ref().join("privkey");

        let privkey = if keypair_path.exists() {
            // load existing key
            let bytes = std::fs::read(&keypair_path)?;
            SecretKey::from_slice(bytes.as_slice())?
        } else {
            // create and save a new key
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let random: [u8; 32] = rng.gen();
            let privkey = SecretKey::from_slice(&random)?;
            std::fs::write(&keypair_path, privkey.secret_bytes())?;
            privkey
        };

        let pubkey = secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &privkey);

        Ok(Self { privkey, pubkey })
    }
}

impl Wallet for SimpleWallet {
    fn address(&self) -> Result<Option<Address>> {
        Ok(Some(Address::from_pubkey(self.pubkey.serialize())))
    }

    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall> {
        use secp256k1::hashes::sha256;
        let secp = secp256k1::Secp256k1::new();
        let msg = secp256k1::Message::from_hashed_data::<sha256::Hash>(call_bytes);
        let sig = secp.sign_ecdsa(&msg, &self.privkey).serialize_compact();

        Ok(SignerCall {
            call_bytes: call_bytes.to_vec(),
            signature: Some(sig),
            pubkey: Some(self.pubkey.serialize()),
            sigtype: SigType::Native,
        })
    }
}
