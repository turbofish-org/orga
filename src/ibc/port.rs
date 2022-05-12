use ibc::core::ics26_routing::context::ModuleId;
use ibc::core::{
    ics05_port::{
        capabilities::{Capability, PortCapability},
        context::{CapabilityReader, PortReader},
        error::Error,
    },
    ics24_host::identifier::PortId,
};

use super::Ibc;

impl CapabilityReader for Ibc {
    fn authenticate_capability(
        &self,
        name: &ibc::core::ics05_port::capabilities::CapabilityName,
        capability: &Capability,
    ) -> Result<(), Error> {
        todo!()
    }

    fn get_capability(
        &self,
        name: &ibc::core::ics05_port::capabilities::CapabilityName,
    ) -> Result<Capability, Error> {
        todo!()
    }
}

impl PortReader for Ibc {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<(ModuleId, PortCapability), Error> {
        todo!()
    }
}
