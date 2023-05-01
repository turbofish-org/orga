use std::borrow::Borrow;

use ibc::applications::transfer::MODULE_ID_STR;
use ibc::core::{
    context::Router,
    ics24_host::identifier::PortId,
    ics26_routing::context::{Module, ModuleId},
};

use super::Ibc;

impl Router for Ibc {
    fn get_route(&self, module_id: &ModuleId) -> Option<&dyn Module> {
        (Borrow::<str>::borrow(module_id) == MODULE_ID_STR).then(|| self as _)
    }

    fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn Module> {
        (Borrow::<str>::borrow(module_id) == MODULE_ID_STR).then(|| self as _)
    }

    fn has_route(&self, module_id: &ModuleId) -> bool {
        self.get_route(module_id).is_some()
    }

    fn lookup_module_by_port(&self, port_id: &PortId) -> Option<ModuleId> {
        let transfer_port = PortId::transfer();
        let transfer_module_id: ModuleId = MODULE_ID_STR.parse().unwrap();

        if port_id == &transfer_port {
            Some(transfer_module_id)
        } else {
            None
        }
    }
}
