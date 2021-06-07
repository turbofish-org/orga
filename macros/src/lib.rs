use proc_macro::TokenStream;

mod state;

#[proc_macro_derive(State)]
pub fn state(item: TokenStream) -> TokenStream {
    state::derive(item)
}
