use ibc::core::ics05_port::context::PortReader;
use ibc::core::ics26_routing::context::ModuleId;
use ibc::core::{ics05_port::error::Error, ics24_host::identifier::PortId};

use super::Ibc;

impl PortReader for Ibc {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<ModuleId, Error> {
        todo!()
    }
}
