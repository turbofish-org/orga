use proc_macro::{self, TokenStream};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

fn derive_named_struct(data: syn::DataStruct, ident: syn::Ident) -> TokenStream {
    let match_blocks: Vec<TokenStream2> = data
        .fields
        .iter()
        .rev()
        .map(|field| {
            let field_name = field.ident.clone().unwrap();
            let field_type = field.ty.clone();
            quote!(
                match <#field_type>::next(&self.#field_name) {
                    Some(new_value) => {
                        return_struct.#field_name = new_value;
                        return Some(return_struct);
                    },
                    None => return_struct.#field_name = Default::default()
                };
            )
        })
        .collect();

    let field_names: Vec<syn::Ident> = data
        .fields
        .iter()
        .map(|field| field.ident.clone().unwrap())
        .collect();

    let output = quote! {
        impl ::orga::collections::Next for #ident {
            fn next(&self) -> Option<#ident> {
                let mut return_struct = Self {
                    #(#field_names: self.#field_names, )*
                };

                #(
                    #match_blocks
                )*
                None
            }
        }
    };
    output.into()
}

pub fn derive(input: TokenStream) -> TokenStream {
    let DeriveInput { ident, data, .. } = parse_macro_input!(input);

    match data.clone() {
        syn::Data::Struct(data) => match data.clone().fields {
            syn::Fields::Named(_) => {
                return derive_named_struct(data, ident);
            }
            syn::Fields::Unnamed(_) => {
                todo!("Tuple struts are not yet supported")
            }
            syn::Fields::Unit => {
                todo!("Unit structs are not supported")
            }
        },
        _ => todo!("Currently only structs are supported"),
    };
}
