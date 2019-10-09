#[macro_use] extern crate failure;

mod error;
mod store;
mod state_machine;

pub use state_machine::*;

