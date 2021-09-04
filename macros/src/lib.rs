#![feature(proc_macro_span)]

use proc_macro::TokenStream;

mod entry;
mod next;
mod state;
mod query;

#[proc_macro_derive(State)]
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
    
#[proc_macro_derive(Next)]
pub fn next(item: TokenStream) -> TokenStream {
    next::derive(item)
}
