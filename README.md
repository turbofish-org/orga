# orga

*Deterministic state machine engine written in Rust*

![CI](https://github.com/nomic-io/orga/actions/workflows/ci.yml/badge.svg)
[![codecov](https://codecov.io/gh/nomic-io/orga/branch/develop/graph/badge.svg?token=ZYA7B56825)](https://codecov.io/gh/nomic-io/orga)
[![Crate](https://img.shields.io/crates/v/orga.svg)](https://crates.io/crates/orga)
[![API](https://docs.rs/orga/badge.svg)](https://docs.rs/orga)

Orga is a stack for building blockchain applications powered by [Tendermint](https://github.com/tendermint/tendermint) consensus.

**Status:** Orga is not ready for production applications, but is in rapid development. Some APIs are subject to change.

## Module Status

| Module            | Description                                                                                                          | Completeness                                                                                                                                                                                                                   | API Stability                                                   |
|-------------------|----------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------|
| [ed](https://github.com/nomic-io/ed)                           | Minimalist traits for fast, deterministic encoding/decoding                                            | Provides `Encode` and `Decode` traits, with implementations for many built-in types (integers, `Vec<T: Encode + Decode>`, etc.). Will likely add tools for easier handmade encodings and composable encoding types (e.g. length-prefixed arrays). | Unlikely to change.                                             |
| [ed_derive](https://github.com/nomic-io/ed/tree/master/derive) | Derive macros for `ed::Encode` and `ed::Decode`                                                                      | Derive macros are implemented for structs. Still needs enum support.                                                                                                                                                                              | Can not change (only provides derive macros).                   |
| orga::abci        | Integration with ABCI (gated by `abci` feature)                                                                      | Implements ABCI app abstraction with serial tx processing. Still needs full ABCI pipeline for parallel tx processing.                                                                                                          | Likely to change significantly.                                 |
| orga::collections | State data structures which implement `orga::state::State` trait                                                     | Implements Map, Set, Deque. Will likely add more.                                                                                                                                                                              | May change significantly as we explore different paradigms.     |
| orga::merkstore   | Integration with [merk](https://github.com/nomic-io/merk) (gated by `merk` feature)                                  | Implements `orga::store::Store` trait for Merk storage, and implements `abci::ABCIStore` so it can be used in an ABCI app. Will grow as `orga::store` grows, e.g. implementing `orga::store::Iter` to iterate through entries. | Unlikely to change beyond changes in `orga::store`.             |
| orga::state       | Traits for representing state data using higher-level abstractions (on top of a `orga::store::Store` implementation) | Implements base `State` trait, and basic implementations of it such as `Value<T>`.                                                                                                                                             | May change significantly as we explore different paradigms.     |
| orga::store       | Traits and implementations for low-level key/value store abstraction                                                 | Implements base `Store` trait, and many composable implementations such as `MapStore`, `NullStore`, `Prefixed`, etc. Will likely add more composable pieces.                                                                   | The base traits may change minorly, overall paradigm is stable. |
| orga_macros       | Macros for Orga traits.                                                                                              | Implements `#[state]` macro for combining `orga::state::State` implementations into struct hierarchies. Currently only supports normal structs, will likely support e.g. enums.                                                | Unlikely to change.                                             |

## Project Goals
- *Performance* - To serve a large user base, blockchains need to be engineered for high throughput, e.g. 10k+ transactions per second. Orga is engineered for maximum concurrency and with the ability to use the right data structures.
- *Simplicity* - Keeping complexity under control makes it easier to understand the system, prevent flaws, and introduce new functionality. When thinking something through, a good heuristic is to choose the solution with fewer lines of code or smaller compiler output.
- *Ease of use* - In our earlier work on [`LotionJS`](https://github.com/nomic-io/lotion), we discovered that blockchain development can be fast and pleasant with the right abstractions. We aim to replicate this experience in Orga.
- *Idiomatic Rust* - When figuring out how to do something, we can often answer it by asking "what would the Rust standard library do?".
