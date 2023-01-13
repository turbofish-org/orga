#![feature(proc_macro_span)]

use proc_macro::TokenStream;

mod call;
mod client;
mod describe;
mod encoding;
mod entry;
mod migrate_from;
mod next;
mod orga;
mod query;
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

#[proc_macro_derive(Query)]
pub fn derive_query(item: TokenStream) -> TokenStream {
    query::derive(item)
}

#[proc_macro_attribute]
pub fn query(args: TokenStream, input: TokenStream) -> TokenStream {
    query::attr(args, input)
}

#[proc_macro_derive(Call, attributes(call))]
pub fn derive_call(item: TokenStream) -> TokenStream {
    call::derive(item)
}

#[proc_macro_attribute]
pub fn call(args: TokenStream, input: TokenStream) -> TokenStream {
    call::attr(args, input)
}

#[proc_macro_derive(Client)]
pub fn derive_client(item: TokenStream) -> TokenStream {
    client::derive(item)
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
