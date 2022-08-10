use super::Ibc;
use crate::encoding::{Decode, Encode, Terminated};
use crate::state::State;
use crate::Result;
use cosmrs::Tx;
use ibc::applications::transfer::msgs::transfer::MsgTransfer;
use ibc::core::ics26_routing::context::{Ics26Context, Module, ModuleId, Router};
use ibc::core::ics26_routing::msgs::Ics26Envelope;
use ibc_proto::google::protobuf::Any;
use std::borrow::Borrow;
use std::convert::TryFrom;

impl Router for Ibc {
    fn get_route_mut(&mut self, module_id: &impl Borrow<ModuleId>) -> Option<&mut dyn Module> {
        let module_id: &ModuleId = module_id.borrow();
        let module_id: &str = module_id.borrow();

        match module_id {
            "transfer" => Some(&mut self.transfers),
            _ => None,
        }
    }

    fn has_route(&self, module_id: &impl Borrow<ModuleId>) -> bool {
        let module_id: &ModuleId = module_id.borrow();
        let module_id: &str = module_id.borrow();

        matches!(module_id, "transfer")
    }
}

impl Ics26Context for Ibc {
    type Router = Self;
    fn router(&self) -> &Self::Router {
        self
    }

    fn router_mut(&mut self) -> &mut Self::Router {
        self
    }
}

impl Encode for IbcTx {
    fn encoding_length(&self) -> ed::Result<usize> {
        unimplemented!()
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        unimplemented!()
    }
}

impl Decode for IbcTx {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        Self::try_from(bytes.as_slice()).map_err(|_| ed::Error::UnexpectedByte(0))
    }
}

impl ed::Terminated for IbcTx {}

impl TryFrom<&[u8]> for IbcTx {
    type Error = crate::Error;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let tx = Tx::from_bytes(bytes)
            .map_err(|_| crate::Error::Ibc("Invalid ICS-26 transaction bytes".into()))?;
        let messages = tx
            .body
            .messages
            .into_iter()
            .map(|msg| {
                let msg = Any {
                    type_url: msg.type_url,
                    value: msg.value,
                };

                if let Ok(msg) = Ics26Envelope::try_from(msg.clone()) {
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

#[derive(Debug)]
pub struct IbcTx(pub Vec<IbcMessage>);

#[derive(Debug, Clone)]
pub enum IbcMessage {
    Ics20(MsgTransfer),
    Ics26(Ics26Envelope),
}
