//! Compatibility with Cosmos SDK transactions.
use orga_macros::orga;

use crate::call::Call as CallTrait;
use crate::coins::{Address, Symbol};

use crate::encoding::{Decode, Encode};

use crate::migrate::MigrateFrom;
use crate::state::State;
use crate::{Error, Result};

use std::marker::PhantomData;

/// The maximum size of a call in bytes.
pub const MAX_CALL_SIZE: usize = 65_535;
/// The flag for a native call.
pub const NATIVE_CALL_FLAG: u8 = 0xff;

/// A plugin for compatibility with Cosmos SDK transactions.
///
/// Types may implement [`ConvertSdkTx`] to define how their native call types
/// can be converted from Cosmos SDK transactions.
///
/// This plugin's call implementation first converts an SDK transaction to a
/// native call if required, then passes the native call along to its inner
/// value.
#[orga(skip(Call), version = 1)]
pub struct SdkCompatPlugin<S, T> {
    pub(crate) symbol: PhantomData<S>,
    /// The inner value.
    pub inner: T,
}

/// A Cosmos SDK transaction or native call.
#[derive(Debug)]
pub enum Call<T> {
    /// A native call.
    Native(T),
    /// A Cosmos SDK transaction.
    Sdk(sdk::Tx),
}

unsafe impl<T> Send for Call<T> {}

impl<T: Encode> Encode for Call<T> {
    fn encoding_length(&self) -> ed::Result<usize> {
        match self {
            Call::Native(native) => Ok(native.encoding_length()? + 1),
            Call::Sdk(tx) => tx.encoding_length(),
        }
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match self {
            Call::Native(native) => {
                NATIVE_CALL_FLAG.encode_into(dest)?;
                native.encode_into(dest)
            }
            Call::Sdk(tx) => tx.encode_into(dest),
        }
    }
}

impl<T: Decode> Decode for Call<T> {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        if bytes.len() > MAX_CALL_SIZE {
            return Err(ed::Error::UnexpectedByte(0));
        }

        match bytes.first() {
            Some(&NATIVE_CALL_FLAG) => {
                let native = T::decode(&bytes.as_slice()[1..])?;
                Ok(Call::Native(native))
            }
            Some(_) => {
                let tx = sdk::Tx::decode(bytes.as_slice())?;
                Ok(Call::Sdk(tx))
            }
            None => {
                let io_err = std::io::ErrorKind::UnexpectedEof.into();
                Err(ed::Error::IOError(io_err))
            }
        }
    }
}

pub mod sdk {
    //! Cosmos SDK types.
    use super::{Address, Decode, Encode, Error, Result, MAX_CALL_SIZE};
    use cosmrs::proto::cosmos::tx::v1beta1::Tx as ProtoTx;
    use prost::Message;
    use serde::{Deserialize, Serialize};
    use std::io::{Error as IoError, ErrorKind};

    /// A Cosmos SDK transaction.
    #[derive(Debug, Clone)]
    pub enum Tx {
        /// An Amino transaction.
        Amino(AminoTx),
        /// A Protobuf transaction.
        Protobuf(cosmrs::Tx),
    }

    impl Encode for Tx {
        fn encoding_length(&self) -> ed::Result<usize> {
            match self {
                Tx::Amino(tx) => {
                    let bytes = serde_json::to_vec(tx).map_err(|_| ed::Error::UnexpectedByte(0))?;
                    Ok(bytes.len())
                }
                Tx::Protobuf(tx) => {
                    let tx: ProtoTx = tx.clone().into();
                    Ok(tx.encoded_len())
                }
            }
        }

        fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
            let bytes = match self {
                Tx::Amino(tx) => {
                    serde_json::to_vec(tx).map_err(|_| ed::Error::UnexpectedByte(0))?
                }
                Tx::Protobuf(tx) => {
                    let tx: ProtoTx = tx.clone().into();
                    tx.encode_to_vec()
                }
            };

            if bytes.len() > MAX_CALL_SIZE {
                return Err(ed::Error::UnexpectedByte(0));
            }

            dest.write_all(&bytes)?;
            Ok(())
        }
    }

    impl Decode for Tx {
        fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
            let mut bytes = Vec::with_capacity(MAX_CALL_SIZE);
            reader.read_to_end(&mut bytes)?;

            if bytes.len() > MAX_CALL_SIZE || bytes.is_empty() {
                return Err(ed::Error::UnexpectedByte(0));
            }

            if b'{' == bytes[0] {
                let tx = serde_json::from_slice(bytes.as_slice())
                    .map_err(|_| ed::Error::UnexpectedByte(123))?;
                return Ok(Tx::Amino(tx));
            }

            let tx = cosmrs::Tx::from_bytes(bytes.as_slice())
                .map_err(|e| IoError::new(ErrorKind::InvalidData, e))?;
            Ok(Tx::Protobuf(tx))
        }
    }

    /// An Amino transaction.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct AminoTx {
        /// The messages in the transaction.
        pub msg: Vec<Msg>,
        /// The fee.
        pub fee: Fee,
        /// The tx memo.
        pub memo: String,
        /// Provided signatures.
        pub signatures: Vec<Signature>,
    }

    impl Tx {
        /// Returns the bytes that must be signed for this transaction.
        pub fn sign_bytes(&self, chain_id: String, nonce: u64) -> Result<Vec<u8>> {
            match self {
                Tx::Amino(tx) => {
                    let sign_tx = SignDoc {
                        account_number: "0".to_string(),
                        chain_id,
                        fee: tx.fee.clone(),
                        memo: tx.memo.clone(),
                        msgs: tx.msg.clone(),
                        sequence: nonce.to_string(),
                    };

                    serde_json::to_vec(&sign_tx).map_err(|e| Error::App(e.to_string()))
                }
                Tx::Protobuf(tx) => {
                    let tx = tx.clone();
                    let signdoc = cosmrs::tx::SignDoc {
                        body_bytes: tx
                            .body
                            .into_bytes()
                            .map_err(|e| Error::App(e.to_string()))?,
                        auth_info_bytes: tx
                            .auth_info
                            .into_bytes()
                            .map_err(|e| Error::App(e.to_string()))?,
                        chain_id,
                        account_number: 0,
                    };
                    signdoc.into_bytes().map_err(|e| Error::App(e.to_string()))
                }
            }
        }

        /// Returns the public key of the sender of this transaction.
        pub fn sender_pubkey(&self) -> Result<[u8; 33]> {
            let pubkey_vec = match self {
                Tx::Amino(tx) => {
                    let pubkey_b64 = &tx
                        .signatures
                        .first()
                        .ok_or_else(|| Error::App("No signatures provided".to_string()))?
                        .pub_key
                        .value;
                    use base64::Engine;
                    base64::prelude::BASE64_STANDARD
                        .decode(pubkey_b64)
                        .map_err(|e| Error::App(e.to_string()))?
                }
                Tx::Protobuf(tx) => tx
                    .auth_info
                    .signer_infos
                    .first()
                    .ok_or_else(|| Error::App("No auth info provided".to_string()))?
                    .public_key
                    .as_ref()
                    .ok_or_else(|| Error::App("No public key provided".to_string()))?
                    .single()
                    .ok_or_else(|| Error::App("Invalid public key".to_string()))?
                    .to_bytes(),
            };

            let mut pubkey_arr = [0; 33];
            pubkey_arr.copy_from_slice(&pubkey_vec);

            Ok(pubkey_arr)
        }

        /// Returns the [Address] of the sender of this transaction.
        pub fn sender_address(&self) -> Result<Address> {
            let signer_call = super::super::signer::sdk_to_signercall(self)?;
            signer_call.address()
        }

        /// Returns the signature of the sender of this transaction.
        pub fn signature(&self) -> Result<[u8; 64]> {
            let sig_vec = match self {
                Tx::Amino(tx) => {
                    let sig_b64 = &tx
                        .signatures
                        .first()
                        .ok_or_else(|| Error::App("No signatures provided".to_string()))?
                        .signature;

                    use base64::Engine;
                    base64::prelude::BASE64_STANDARD
                        .decode(sig_b64)
                        .map_err(|e| Error::App(e.to_string()))?
                }
                Tx::Protobuf(tx) => tx
                    .signatures
                    .first()
                    .ok_or_else(|| Error::App("No signatures provided".to_string()))?
                    .clone(),
            };

            let mut sig_arr = [0; 64];
            sig_arr.copy_from_slice(&sig_vec);

            Ok(sig_arr)
        }

        /// Returns the signature type of the sender of this transaction if it
        /// is an Amino transaction.
        ///
        /// Returns `None` if the transaction is a Protobuf transaction.
        pub fn sig_type(&self) -> Result<Option<&str>> {
            Ok(match self {
                Tx::Amino(tx) => tx
                    .signatures
                    .first()
                    .ok_or_else(|| Error::App("No signatures provided".to_string()))?
                    .r#type
                    .as_deref(),

                Tx::Protobuf(_) => None,
            })
        }
    }

    /// Cosmos SDK sign doc.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct SignDoc {
        /// String representation of the account number.
        pub account_number: String,
        /// The chain ID.
        pub chain_id: String,
        /// The fee.
        pub fee: Fee,
        /// The tx memo.
        pub memo: String,
        /// The messages in the transaction.
        pub msgs: Vec<Msg>,
        /// String representation of the account sequence number.
        pub sequence: String,
    }

    /// Cosmos SDK message.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Msg {
        /// The type of the message.
        #[serde(rename = "type")]
        pub type_: String,
        /// The JSON value of the message.
        pub value: serde_json::Value,
    }

    /// Cosmos SDK fee.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Fee {
        /// The amount of coins to pay for the transaction.
        pub amount: Vec<Coin>,
        /// Tx gas setting.
        pub gas: String,
    }

    /// Cosmos SDK coin.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Coin {
        /// String representation of the amount.
        pub amount: String,
        /// The type of coin.
        pub denom: String,
    }

    /// Cosmos SDK signature.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Signature {
        /// The public key of the signer.
        pub pub_key: PubKey,
        /// The base64-encoded signature.
        pub signature: String,
        /// The type of signature.
        pub r#type: Option<String>,
    }

    /// Cosmos SDK public key.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct PubKey {
        /// The type of public key.
        #[serde(rename = "type")]
        pub type_: String,
        /// The base64-encoded public key.
        pub value: String,
    }

    /// Cosmos SDK token transfer message.
    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgSend {
        /// The sender's address.
        pub from_address: String,
        /// The receiver's address.
        pub to_address: String,
        /// The coins to transfer.
        pub amount: Vec<Coin>,
    }

    /// Cosmos SDK delegation message.
    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgDelegate {
        /// The delegator's address.
        pub delegator_address: String,
        /// The validator's address.
        pub validator_address: String,
        /// The coin to delegate.
        pub amount: Option<Coin>,
    }

    /// Cosmos SDK redelegation message.
    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgBeginRedelegate {
        /// The delegator's address.
        pub delegator_address: String,
        /// The source validator's address.
        pub validator_src_address: String,
        /// The destination validator's address.
        pub validator_dst_address: String,
        /// The coin to redelegate.
        pub amount: Option<Coin>,
    }

    /// Cosmos SDK undelegation message.
    #[derive(Deserialize, Debug, Clone)]
    pub struct MsgUndelegate {
        /// The delegator's address.
        pub delegator_address: String,
        /// The validator's address.
        pub validator_address: String,
        /// The coin to undelegate.
        pub amount: Option<Coin>,
    }
}

/// A trait for converting Cosmos SDK transactions to native calls.
pub trait ConvertSdkTx {
    /// The type returned by the conversion, usually the type's native call.
    type Output;

    /// Convert the given SDK transaction to a native call.
    fn convert(&self, msg: &sdk::Tx) -> Result<Self::Output>;
}

impl<S: Symbol, T> CallTrait for SdkCompatPlugin<S, T>
where
    T: CallTrait + State + ConvertSdkTx<Output = T::Call>,
{
    type Call = Call<T::Call>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let call = match call {
            Call::Native(call) => call,
            Call::Sdk(tx) => self.inner.convert(&tx)?,
        };

        self.inner.call(call)
    }
}

impl<S: 'static, T: State> MigrateFrom<SdkCompatPluginV0<S, T>> for SdkCompatPluginV1<S, T> {
    fn migrate_from(_value: SdkCompatPluginV0<S, T>) -> Result<Self> {
        unreachable!()
    }
}

#[cfg(feature = "abci")]
mod abci {
    use super::super::*;
    use super::*;
    use crate::abci::{BeginBlock, EndBlock, InitChain};

    impl<S, T: State> BeginBlock for SdkCompatPlugin<S, T>
    where
        T: BeginBlock + State,
    {
        fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
            self.inner.begin_block(ctx)
        }
    }

    impl<S, T: State> EndBlock for SdkCompatPlugin<S, T>
    where
        T: EndBlock + State,
    {
        fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
            self.inner.end_block(ctx)
        }
    }

    impl<S, T: State> InitChain for SdkCompatPlugin<S, T>
    where
        T: InitChain + State,
    {
        fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
            self.inner.init_chain(ctx)
        }
    }

    impl<S, T> crate::abci::AbciQuery for SdkCompatPlugin<S, T>
    where
        T: crate::abci::AbciQuery + State + CallTrait,
    {
        fn abci_query(
            &self,
            request: &tendermint_proto::v0_34::abci::RequestQuery,
        ) -> Result<tendermint_proto::v0_34::abci::ResponseQuery> {
            self.inner.abci_query(request)
        }
    }
}
