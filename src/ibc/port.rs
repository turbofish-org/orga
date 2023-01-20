use crate::collections::Map;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};
use crate::migrate::MigrateFrom;
use crate::orga;
use crate::state::State;
use ibc::core::ics05_port::context::PortReader;
use ibc::core::ics26_routing::context::ModuleId;
use ibc::core::{ics05_port::error::Error, ics24_host::identifier::PortId};
use serde::{Deserialize, Serialize};

use super::{Adapter, Ibc};

#[orga]
pub struct PortStore {
    module_by_port: Map<Adapter<PortId>, Adapter<ModuleId>>,
}

impl PortReader for Ibc {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<ModuleId, Error> {
        match port_id.as_str() {
            "transfer" => Ok("transfer".parse().unwrap()),
            _ => Err(Error::unknown_port(port_id.clone())),
        }
    }
}
