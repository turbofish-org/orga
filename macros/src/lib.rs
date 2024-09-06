#![feature(proc_macro_span)]
#![feature(box_patterns)]

use proc_macro::TokenStream;

mod build_call;
mod channels;
mod child;
mod describe;
mod encoding;
mod entry;
mod field_call;
mod field_query;
mod method_call;
mod method_query;
mod migrate;
mod next;
mod orga;
mod state;
mod utils;

#[proc_macro_derive(State, attributes(state))]
pub fn derive_state(item: TokenStream) -> TokenStream {
    state::derive(item)
}

#[proc_macro_derive(Entry, attributes(key))]
pub fn derive_entry(item: TokenStream) -> TokenStream {
    entry::derive(item)
}

#[proc_macro_derive(Next)]
pub fn derive_next(item: TokenStream) -> TokenStream {
    next::derive(item)
}

#[proc_macro_derive(Describe)]
pub fn derive_describe(item: TokenStream) -> TokenStream {
    describe::derive(item)
}

#[proc_macro_derive(Migrate, attributes(migrate))]
pub fn derive_migrate(item: TokenStream) -> TokenStream {
    migrate::derive(item)
}

/// High-level attribute macro for orga types.
///
/// This macro manages versions, channels, and adds implementations of the
/// following traits:
///
/// - `Encode`
/// - `Decode`
/// - `serde::Serialize`
/// - `Default`
/// - [Migrate]
/// - [FieldCall]
/// - [FieldQuery]
/// - [Describe]
/// - [State]
///
/// ## Versions
///
/// The `version` parameter sets the version range for the type. If not set,
/// only version 0 of the type is generated.
///
/// If a single number is provided (e.g. `#[orga(version = 1)]`), it is treated
/// as the inclusive upper bound of the version range.
///
/// An inclusive version range may also be specified if older versions are not
/// required, e.g. `#[orga(version = 2..=4)]` will only generate types for
/// versions 2, 3, and 4.
///
/// For each version of the type, a separate struct is generated, named by
/// appending the version number to the end of the type name. For example, a
/// type `Foo` with `#[orga(version = 2)]` will generate `FooV0`, `FooV1`, and
/// `Foo` structs, with the highest version maintaining the original type's
/// name.
///
/// Fields may use `#[orga(version(V0, V1))]` to restrict the versions in which
/// a field definition should be present (all versions if not specified).
///
/// Implementations of `Encode`, `Decode`, and [State] are provided for all
/// types in the version range, each of which prepend a version number to their
/// respective encodings.
///
/// An implementation of [Migrate] is provided by default which assumes an
/// implementation of `MigrateFrom` for each version from its predecessor.
///
/// [FieldCall], [FieldQuery], and [Describe] implementations are provided only
/// for the last version in the range.
///
/// Features may be disabled ad-hoc using the `skip` parameter, e.g.
/// `#[orga(skip(Migrate))]` to enable hand implementation.
///
/// ## Attributes
///
/// Attributes from other macros to be aware of when using `#[orga]`:
///
/// - `#[call]` to annotate a callable field.
/// - `#[state]` for custom prefixing.
///
/// ## Impl blocks
///
/// `#[orga]` may also be used on an impl block for a type to generate a
/// `MethodCall` and/or `MethodQuery` implementation for the type. Public
/// methods with a mutable receiver will be used to generate a [MethodCall]
/// implementation, and public methods with an immutable receiver will be used
/// to generate a [MethodQuery] implementation.
///
/// Method calls and queries may be generated in the same impl block or
/// different blocks, but all method calls must be defined in one impl,
/// as must all method queries.
#[proc_macro_attribute]
pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    orga::orga(args, input)
}

/// Attribute macro for creating variations of a type or impl block.
///
/// Supports gating functionality on a per-variant basis. Useful for e.g.
/// maintaining separate `Testnet` and `Mainnet` channels without requiring
/// separate binaries.
#[proc_macro_attribute]
pub fn channels(args: TokenStream, input: TokenStream) -> TokenStream {
    channels::channels(args, input)
}

#[proc_macro_derive(VersionedEncoding, attributes(encoding))]
pub fn derive_versioned_encoding(item: TokenStream) -> TokenStream {
    encoding::derive(item)
}

#[proc_macro_derive(FieldCall, attributes(call))]
pub fn derive_field_call(item: TokenStream) -> TokenStream {
    field_call::derive(item)
}

#[proc_macro_attribute]
pub fn call_block(item: TokenStream, input: TokenStream) -> TokenStream {
    method_call::call_block(item, input)
}

#[proc_macro_derive(Child)]
pub fn derive_child(item: TokenStream) -> TokenStream {
    child::derive(item)
}

#[proc_macro]
pub fn build_call(input: TokenStream) -> TokenStream {
    build_call::build_call(input)
}

#[proc_macro]
pub fn const_ident(input: TokenStream) -> TokenStream {
    child::const_ident(input)
}

#[proc_macro_derive(FieldQuery, attributes(query))]
pub fn derive_field_query(item: TokenStream) -> TokenStream {
    field_query::derive(item)
}

#[proc_macro_attribute]
pub fn query_block(item: TokenStream, input: TokenStream) -> TokenStream {
    method_query::query_block(item, input)
}
