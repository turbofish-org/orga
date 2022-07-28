use crate::collections::Map;
use crate::state::State;
use ibc::core::ics05_port::context::PortReader;
use ibc::core::ics26_routing::context::ModuleId;
use ibc::core::{ics05_port::error::Error, ics24_host::identifier::PortId};

use super::{Adapter, Ibc};

#[derive(State)]
pub struct PortStore {
    module_by_port: Map<Adapter<PortId>, Adapter<ModuleId>>,
}

impl PortReader for Ibc {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<ModuleId, Error> {
        println!("lookup module by port: {}", port_id);
        match port_id.as_str() {
            "transfer" => Ok("transfer".parse().unwrap()),
            _ => Err(Error::unknown_port(port_id.clone())),
        }
        // self.ports
        //     .module_by_port
        //     .get(port_id.clone().into())
        //     .map_err(|_| Error::implementation_specific())?
        //     .map(|v| v.clone().into_inner())
        //     .ok_or_else(|| Error::unknown_port(port_id.clone()))
    }
}
