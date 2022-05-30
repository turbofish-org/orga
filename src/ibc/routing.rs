use super::Ibc;
use crate::encoding::{Decode, Encode, Terminated};
use crate::Result;
use cosmrs::Tx;
use ibc::core::ics26_routing::context::{Ics26Context, Module, ModuleId, Router};
use ibc::core::ics26_routing::msgs::Ics26Envelope;
use ibc_proto::google::protobuf::Any;
use std::borrow::Borrow;
use std::convert::TryFrom;

impl Router for Ibc {
    fn get_route_mut(&mut self, module_id: &impl Borrow<ModuleId>) -> Option<&mut dyn Module> {
        todo!()
    }

    fn has_route(&self, module_id: &impl Borrow<ModuleId>) -> bool {
        todo!()
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

#[derive(Debug)]
pub struct Ics26Message(pub Vec<Ics26Envelope>);

impl Encode for Ics26Message {
    fn encoding_length(&self) -> ed::Result<usize> {
        unimplemented!()
    }

    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        unimplemented!()
    }
}

impl Decode for Ics26Message {
    fn decode<R: std::io::Read>(mut reader: R) -> ed::Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;

        Self::try_from(bytes.as_slice()).map_err(|_| ed::Error::UnexpectedByte(0))
    }
}

impl ed::Terminated for Ics26Message {}

impl TryFrom<&[u8]> for Ics26Message {
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

                Ics26Envelope::try_from(msg)
                    .map_err(|_| crate::Error::Ibc("Invalid ICS-26 message".into()))
            })
            .collect::<Result<Vec<Ics26Envelope>>>()?;

        Ok(Self(messages))
    }
}
