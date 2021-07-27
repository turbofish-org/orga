use proc_macro::TokenStream;

mod entry;
mod state;

#[proc_macro_derive(State)]
pub fn state(item: TokenStream) -> TokenStream {
    state::derive(item)
}

#[proc_macro_derive(Entry)]
pub fn entry(item: TokenStream) -> TokenStream {
    entry::derive(item)
}
