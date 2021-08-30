use crate::call::Call;
use crate::encoding::{Decode, Encode, Terminated};
use crate::Result;
use tendermint::public_key::{Ed25519, PublicKey};
use tendermint::signature::Ed25519Signature;

#[derive(Encode, Decode)]
pub struct Transaction {
    pub signature: Option<[u8; 64]>,
    pub pubkey: Option<[u8; 32]>,
    pub call_bytes: Vec<u8>,
}

impl Transaction {
    fn as_call<T: Call + Encode>(call: &T) -> Result<Self> {
        let call_bytes = Encode::encode(call)?;

        Ok(Self {
            call_bytes,
            signature: None,
            pubkey: None,
        })
    }

    pub fn signer(&self) -> Result<Option<[u8; 32]>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_tx() {
        let tx = Transaction {
            signature: None,
            pubkey: None,
            call_bytes: vec![1, 2, 3],
        };
        assert!(tx.signer().unwrap().is_none());
    }

    #[test]
    #[should_panic(expected = "Malformed transaction")]
    fn malformed_tx_no_signature() {
        let tx = Transaction {
            signature: None,
            pubkey: Some([1; 32]),
            call_bytes: vec![1, 2, 3],
        };
        tx.signer().unwrap();
    }

    #[test]
    #[should_panic(expected = "Malformed transaction")]
    fn malformed_tx_no_pubkey() {
        let tx = Transaction {
            signature: Some([1; 64]),
            pubkey: None,
            call_bytes: vec![1, 2, 3],
        };
        tx.signer().unwrap();
    }

    #[test]
    #[should_panic]
    fn invalid_signature() {
        let tx = Transaction {
            signature: Some([1; 64]),
            pubkey: Some([1; 32]),
            call_bytes: vec![1, 2, 3],
        };
        tx.signer().unwrap();
    }

    #[test]
    fn valid_signature() {
        let pubkey = [
            59, 106, 39, 188, 206, 182, 164, 45, 98, 163, 168, 208, 42, 111, 13, 115, 101, 50, 21,
            119, 29, 226, 67, 166, 58, 192, 72, 161, 139, 89, 218, 41,
        ];
        let tx = Transaction {
            signature: Some([
                42, 38, 119, 155, 166, 203, 181, 229, 66, 146, 37, 127, 114, 90, 241, 18, 178, 115,
                195, 135, 40, 50, 150, 130, 217, 158, 216, 27, 166, 215, 103, 3, 80, 174, 76, 197,
                60, 84, 86, 250, 67, 113, 40, 209, 146, 152, 165, 217, 73, 171, 70, 227, 212, 26,
                179, 219, 207, 176, 179, 92, 137, 94, 147, 4,
            ]),
            pubkey: Some(pubkey),
            call_bytes: vec![1, 2, 3],
        };

        assert_eq!(tx.signer().unwrap().unwrap(), pubkey);
    }
}
