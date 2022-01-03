use ibc::core::{
    ics05_port::{capabilities::Capability, context::PortReader, error::Error},
    ics24_host::identifier::PortId,
};

use super::Ibc;

impl PortReader for Ibc {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<Capability, Error> {
        todo!()
    }

    fn authenticate(&self, key: &Capability, port_id: &PortId) -> bool {
        todo!()
    }
}
