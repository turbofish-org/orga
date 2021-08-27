#![feature(map_first_last)]
#![feature(entry_insert)]
#![feature(min_specialization)]
#![feature(once_cell)]

/// Integration with ABCI (gated by `abci` feature).
#[cfg(feature = "abci")]
pub mod abci;

/// Data structures which implement the [`state::State`](state/trait.State.html)
/// trait.
pub mod collections;

/// Integration with [merk](https://docs.rs/merk) (gated by `merk` feature).
#[cfg(feature = "merk")]
pub mod merk;

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

/// Tendermint process handler.
pub mod tendermint;

mod error;

// re-exports
pub use error::*;
pub use orga_macros as macros;

pub mod prelude {
    #[cfg(feature = "abci")]
    pub use crate::abci::*;
    pub use crate::collections::*;
    pub use crate::state::*;
    pub use crate::store::*;
    pub use crate::Result;
}
