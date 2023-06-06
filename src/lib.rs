#![feature(bound_map)]
#![feature(associated_type_defaults)]
#![feature(trivial_bounds)]
#![allow(incomplete_features)]
#![feature(specialization)]
#![feature(try_trait_v2)]
#![feature(never_type)]
#![feature(adt_const_params)]
#![feature(fn_traits)]
#![feature(async_closure)]
#![feature(local_key_cell_methods)]
#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(lazy_cell)]
#![feature(async_fn_in_trait)]
#![feature(trait_upcasting)]

extern crate self as orga;
pub use orga_macros::orga;

/// Integration with ABCI.
pub mod abci;

pub mod call;

pub mod client;

/// Data structures which implement the [`state::State`](state/trait.State.html)
/// trait.
pub mod collections;

pub mod describe;

/// Traits for deterministic encoding and decoding.
///
/// This module is actually just a re-export of the [ed](https://docs.rs/ed)
/// crate.
pub mod encoding;

/// Integration with [merk](https://docs.rs/merk) (gated by `merk` feature).
#[cfg(feature = "merk")]
pub mod merk;

pub mod query;

/// High-level abstractions for state data.
pub mod state;

/// Helpers for executing state machine logic.
pub mod state_machine;

/// Low-level key/value store abstraction.
pub mod store;

/// Tendermint process handler.
#[cfg(feature = "abci")]
pub mod tendermint;

pub mod migrate;

pub mod plugins;

pub mod coins;

pub mod context;

pub mod upgrade;

mod error;

pub use cosmrs;

mod compat;
pub use compat::{compat_mode, set_compat_mode};

// re-exports
pub use async_trait::async_trait;
pub use educe::Educe;
pub use error::*;
pub use futures_lite::future::Boxed as BoxFuture;
pub use orga_macros as macros;
pub use serde;
pub use serde_json::Value as JsonValue;

pub mod prelude {
    pub use crate::call::{Call, FieldCall, MethodCall};
    pub use crate::encoding::{Decode, Encode, Terminated};
    pub use crate::query::{FieldQuery, MethodQuery, Query};
    pub use crate::state::State;
    pub use crate::store::{Read, Store, Write};
    pub use crate::{Error, Result};
}
