#![feature(proc_macro_span)]
#![feature(box_patterns)]

use proc_macro::TokenStream;

mod build_call;
mod child;
mod describe;
mod encoding;
mod entry;
mod field_call;
mod field_query;
mod method_call;
mod method_query;
mod migrate_from;
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

#[proc_macro_derive(MigrateFrom, attributes(migrate_from))]
pub fn derive_migrate_from(item: TokenStream) -> TokenStream {
    migrate_from::derive(item)
}

#[proc_macro_attribute]
pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    orga::orga(args, input)
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
