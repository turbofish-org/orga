#![feature(trait_alias)]
#![feature(fn_traits)]
#![feature(proc_macro_hygiene)]
#![feature(optin_builtin_traits)]
#![feature(map_first_last)]

/// Integration with ABCI (gated by `abci` feature).
#[cfg(feature = "abci")]
pub mod abci;

/// Data structures which implement the [`state::State`](state/trait.State.html)
/// trait.
pub mod collections;

/// Integration with [merk](https://docs.rs/merk) (gated by `merk` feature).
#[cfg(feature = "merk")]
pub mod merkstore;

/// Traits for deterministic encoding and decoding.
///
/// This module is actually just a re-export of the [ed](https://docs.rs/ed)
/// crate.
pub mod encoding;

/// High-level abstractions for state data.
pub mod state;

/// Helpers for executing state machine logic.
pub mod state_machine;

/// Low-level key/value store abstraction.
pub mod store;

mod error;

// re-exports
pub use error::*;
pub use store::Store;
pub use orga_macros as macros;
