use proc_macro::TokenStream;

mod entry;
mod state;
// mod bindgen;

#[proc_macro_derive(State)]
pub fn state(item: TokenStream) -> TokenStream {
    state::derive(item)
}

#[proc_macro_derive(Entry, attributes(key))]
pub fn entry(item: TokenStream) -> TokenStream {
    entry::derive(item)
}

// #[proc_macro_attribute]
// pub fn orga_bindgen(args: TokenStream, input: TokenStream) -> TokenStream {
//     bindgen::attr(args, input)
// }
