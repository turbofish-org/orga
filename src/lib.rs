#![no_std]

#[cfg(test)]
#[macro_use] extern crate std;

#[macro_use] extern crate failure;

mod error;
mod store;
mod state_machine;

pub use state_machine::*;

