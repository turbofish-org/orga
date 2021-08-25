use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::*;
use quote::quote;

pub fn attr(args: TokenStream, input: TokenStream) -> TokenStream {
  todo!()

  let args = parse_macro_input!(args as AttributeArgs);
  let input = parse_macro_input!(input as ItemFn);

  input.into()
}
