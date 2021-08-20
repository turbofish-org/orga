use proc_macro::TokenStream;

mod entry;
mod next;
mod state;

#[proc_macro_derive(State)]
pub fn state(item: TokenStream) -> TokenStream {
    state::derive(item)
}

#[proc_macro_derive(Entry, attributes(key))]
pub fn entry(item: TokenStream) -> TokenStream {
    entry::derive(item)
}

#[proc_macro_derive(Next)]
pub fn next(item: TokenStream) -> TokenStream {
    state::derive(item)
}
