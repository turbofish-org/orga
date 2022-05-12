use super::Ibc;
use ibc::core::ics26_routing::context::{Ics26Context, Module, ModuleId, Router};
use std::borrow::Borrow;

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
