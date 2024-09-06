use crate::encoding::{Decode, Encode};
use crate::Result;
use cosmrs::proto::cosmos::tx::v1beta1::Tx as TxProto;
use cosmrs::Tx;
use ed::Terminated;
use ibc::apps::transfer::types::msgs::transfer::MsgTransfer;
use ibc::core::handler::types::msgs::MsgEnvelope;
use ibc_proto::google::protobuf::Any;
use prost::Message;

/// Wrapper for [IbcMessage]s for encoding compatibility.
#[derive(Clone, Debug)]
pub struct IbcTx(pub Vec<IbcMessage>);

/// Wrapper for [Tx] for encoding compatibility.
#[derive(Clone, Debug)]
pub struct RawIbcTx(pub Tx);

impl Encode for RawIbcTx {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        let proto_tx = TxProto::from(self.0.clone());
        let bytes = proto_tx.encode_to_vec();

        Ok(dest.write_all(&bytes)?)
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        let proto_tx = TxProto::from(self.0.clone());

        Ok(proto_tx.encoded_len())
    }
}

impl Decode for RawIbcTx {
    fn decode<R: std::io::Read>(mut input: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        input.read_to_end(&mut bytes)?;
        let proto_tx =
            TxProto::decode(bytes.as_slice()).map_err(|_| ed::Error::UnexpectedByte(0))?;

        Ok(Self(
            proto_tx
                .try_into()
                .map_err(|_| ed::Error::UnexpectedByte(0))?,
        ))
    }
}

impl Terminated for RawIbcTx {}

/// Supported IBC messages.
#[derive(Clone, Debug)]
pub enum IbcMessage {
    /// ICS20 transfer message.
    Ics20(MsgTransfer),
    /// ICS26 envelope message.
    Ics26(MsgEnvelope),
}

// impl Encode for IbcTx {
//     fn encode_into<W: std::io::Write>(&self, _dest: &mut W) -> ed::Result<()>
// {         unimplemented!();
//     }

//     fn encoding_length(&self) -> ed::Result<usize> {
//         unimplemented!()
//     }
// }

// impl Decode for IbcTx {
//     fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
//         let mut bytes = vec![];
//         reader.read_to_end(&mut bytes)?;

//         Self::try_from(bytes.as_slice()).map_err(|_|
// ed::Error::UnexpectedByte(0))     }
// }

// impl Terminated for IbcTx {}

impl TryFrom<&[u8]> for IbcTx {
    type Error = crate::Error;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let tx = Tx::from_bytes(bytes)
            .map_err(|_| crate::Error::Ibc("Invalid IBC transaction bytes".into()))?;

        tx.try_into()
    }
}

impl TryFrom<RawIbcTx> for IbcTx {
    type Error = crate::Error;

    fn try_from(raw_tx: RawIbcTx) -> Result<Self> {
        raw_tx.0.try_into()
    }
}

impl TryFrom<Tx> for IbcTx {
    type Error = crate::Error;

    fn try_from(tx: Tx) -> Result<Self> {
        let messages = tx
            .body
            .messages
            .into_iter()
            .map(|msg| {
                let msg = Any {
                    type_url: msg.type_url,
                    value: msg.value,
                };

                if let Ok(msg) = MsgEnvelope::try_from(msg.clone()) {
                    return Ok(IbcMessage::Ics26(msg));
                } else if let Ok(msg) = MsgTransfer::try_from(msg) {
                    return Ok(IbcMessage::Ics20(msg));
                }
                Err(crate::Error::Ibc("Invalid IBC message".into()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self(messages))
    }
}
