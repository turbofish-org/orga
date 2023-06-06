use proc_macro::{self, TokenStream};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::BTreeMap;
use syn::{parse_macro_input, DeriveInput};

fn is_key_field(field: &syn::Field) -> bool {
    let maybe_keys: Vec<String> = field
        .attrs
        .iter()
        .map(|attr| attr.path().get_ident().unwrap().to_string())
        .collect();
    let key = "key";

    maybe_keys.contains(&key.to_string())
}

fn parse_named_struct(data: syn::DataStruct, keys: bool) -> Vec<(syn::Ident, syn::Type)> {
    data.fields
        .iter()
        .filter(|field| !(keys ^ is_key_field(field)))
        .map(|f| (f.ident.clone().unwrap(), f.ty.clone()))
        .collect()
}

fn parse_unnamed_struct(data: syn::DataStruct, keys: bool) -> Vec<(syn::Index, syn::Type)> {
    data.fields
        .iter()
        .enumerate()
        .filter(|(_, field)| !(keys ^ is_key_field(field)))
        .map(|(i, field)| (syn::Index::from(i), field.ty.clone()))
        .collect()
}

fn generate_named_struct_from_body(
    key_field_names: &Vec<syn::Ident>,
    value_field_names: &Vec<syn::Ident>,
) -> Vec<TokenStream2> {
    let self_body: Vec<TokenStream2> = key_field_names
        .iter()
        .enumerate()
        .map(|(i, name)| (i, name, 0))
        .chain(
            value_field_names
                .iter()
                .enumerate()
                .map(|(i, name)| (i, name, 1)),
        )
        .map(|(i, name, position)| {
            let position = syn::Index::from(position);
            let index = syn::Index::from(i);
            quote! { #name: item.#position.#index,}
        })
        .collect();

    self_body
}

fn generate_named_impl_block(
    ident: syn::Ident,
    key_field_names: Vec<syn::Ident>,
    key_field_types: Vec<syn::Type>,
    value_field_names: Vec<syn::Ident>,
    value_field_types: Vec<syn::Type>,
    from_body: Vec<TokenStream2>,
) -> TokenStream {
    let output = quote! {
        impl ::orga::collections::Entry for #ident {
            type Key = (
                #(#key_field_types,)*
            );

            type Value = (
                #(#value_field_types,)*
            );

            fn into_entry(self) -> (Self::Key, Self::Value) {
                (
                    (#(
                        self.#key_field_names,
                    )*),
                    (#(
                        self.#value_field_names,
                    )*),
                )
            }

            fn from_entry(item: (Self::Key, Self::Value)) -> Self {
                Self {
                    #(#from_body)*
                }
            }
        }
    };

    output.into()
}

fn generate_named_one_tuple_from_body(
    key_field_name: &syn::Ident,
    value_field_names: &Vec<syn::Ident>,
) -> Vec<TokenStream2> {
    let key_quote = quote! { #key_field_name: item.0, };
    let mut value_quote: Vec<TokenStream2> = value_field_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let index = syn::Index::from(i);
            quote! { #name: item.1.#index,}
        })
        .collect();
    value_quote.push(key_quote);

    value_quote
}

fn generate_named_one_tuple_impl_block(
    ident: syn::Ident,
    key_field_name: &syn::Ident,
    key_field_type: &syn::Type,
    value_field_names: Vec<syn::Ident>,
    value_field_types: Vec<syn::Type>,
    from_body: Vec<TokenStream2>,
) -> TokenStream {
    let output = quote! {
        impl ::orga::collections::Entry for #ident {
            type Key = #key_field_type;

            type Value = (
                #(#value_field_types,)*
            );

            fn into_entry(self) -> (Self::Key, Self::Value) {
                (
                    self.#key_field_name,
                    (#(
                        self.#value_field_names,
                    )*),
                )
            }

            fn from_entry(item: (Self::Key, Self::Value)) -> Self {
                Self {
                    #(#from_body)*
                }
            }
        }
    };

    output.into()
}

fn derive_named_struct(data: syn::DataStruct, ident: syn::Ident) -> TokenStream {
    let keys = parse_named_struct(data.clone(), true);
    let values = parse_named_struct(data, false);

    let key_field_types: Vec<syn::Type> = keys.iter().map(|key| key.1.clone()).collect();
    let key_field_names: Vec<syn::Ident> = keys.iter().map(|key| key.0.clone()).collect();

    let value_field_types: Vec<syn::Type> = values.iter().map(|value| value.1.clone()).collect();
    let value_field_names: Vec<syn::Ident> = values.iter().map(|value| value.0.clone()).collect();

    match key_field_types.len() {
        0 => panic!("Entry derivation requires at least one key field to be specified."),
        1 => {
            let key_field_name = key_field_names.get(0).unwrap();
            let key_field_type = key_field_types.get(0).unwrap();

            let from_body = generate_named_one_tuple_from_body(&key_field_name, &value_field_names);
            generate_named_one_tuple_impl_block(
                ident,
                key_field_name,
                key_field_type,
                value_field_names,
                value_field_types,
                from_body,
            )
        }
        _ => {
            let from_body = generate_named_struct_from_body(&key_field_names, &value_field_names);
            generate_named_impl_block(
                ident,
                key_field_names,
                key_field_types,
                value_field_names,
                value_field_types,
                from_body,
            )
        }
    }
}

fn generate_unnamed_struct_from_body(
    keys: &Vec<syn::Index>,
    values: &Vec<syn::Index>,
) -> Vec<TokenStream2> {
    let mut field_key_status = BTreeMap::new();

    for key in keys {
        field_key_status.insert(key.index, true);
    }
    for value in values {
        field_key_status.insert(value.index, false);
    }

    let mut num_keys = 0;
    let mut num_vals = 0;

    let output: Vec<TokenStream2> = field_key_status
        .iter()
        .map(|(_, is_key)| match is_key {
            true => {
                let j = syn::Index::from(num_keys);
                num_keys += 1;
                quote! { item.0.#j}
            }
            false => {
                let j = syn::Index::from(num_vals);
                num_vals += 1;
                quote! { item.1.#j}
            }
        })
        .collect();
    output
}

fn generate_unnamed_impl_block(
    ident: syn::Ident,
    key_field_indices: &Vec<syn::Index>,
    key_field_types: Vec<syn::Type>,
    value_field_indices: &Vec<syn::Index>,
    value_field_types: Vec<syn::Type>,
    from_body: Vec<TokenStream2>,
) -> TokenStream {
    let output = quote! {
        impl ::orga::collections::Entry for #ident {
            type Key = (
                #(#key_field_types,)*
            );

            type Value = (
                #(#value_field_types,)*
            );

            fn into_entry(self) -> (Self::Key, Self::Value) {
                (
                    (#(
                        self.#key_field_indices,
                    )*),
                    (#(
                        self.#value_field_indices,
                    )*),
                )
            }

            fn from_entry(item: (Self::Key, Self::Value)) -> Self {
                Self(#(#from_body,)*)
            }
        }
    };

    output.into()
}

fn generate_unnamed_one_tuple_from_body(
    key: &syn::Index,
    values: &Vec<syn::Index>,
) -> Vec<TokenStream2> {
    let mut field_key_status = BTreeMap::new();

    field_key_status.insert(key.index, true);

    for value in values {
        field_key_status.insert(value.index, false);
    }

    let mut num_vals = 0;

    let output: Vec<TokenStream2> = field_key_status
        .iter()
        .map(|(_, is_key)| match is_key {
            true => {
                quote! { item.0}
            }
            false => {
                let j = syn::Index::from(num_vals);
                num_vals += 1;
                quote! { item.1.#j}
            }
        })
        .collect();
    output
}

fn generate_unnamed_one_tuple_impl_block(
    ident: syn::Ident,
    key_field_index: &syn::Index,
    key_field_type: &syn::Type,
    value_field_indices: &Vec<syn::Index>,
    value_field_types: Vec<syn::Type>,
    from_body: Vec<TokenStream2>,
) -> TokenStream {
    let output = quote! {
        impl ::orga::collections::Entry for #ident {
            type Key = #key_field_type;

            type Value = (
                #(#value_field_types,)*
            );

            fn into_entry(self) -> (Self::Key, Self::Value) {
                (
                    self.#key_field_index,
                    (#(
                        self.#value_field_indices,
                    )*),
                )
            }

            fn from_entry(item: (Self::Key, Self::Value)) -> Self {
                Self(#(#from_body,)*)
            }
        }
    };

    output.into()
}

fn derive_unnamed_struct(data: syn::DataStruct, ident: syn::Ident) -> TokenStream {
    let keys = parse_unnamed_struct(data.clone(), true);
    let values = parse_unnamed_struct(data, false);

    let key_field_types: Vec<syn::Type> = keys.iter().map(|key| key.1.clone()).collect();
    let key_field_indices: Vec<syn::Index> = keys.iter().map(|key| key.0.clone()).collect();

    let value_field_types: Vec<syn::Type> = values.iter().map(|value| value.1.clone()).collect();
    let value_field_indices: Vec<syn::Index> = values.iter().map(|value| value.0.clone()).collect();

    match key_field_types.len() {
        0 => panic!("Entry derivation requires at least one key field to be specified."),
        1 => {
            let key_field_index = key_field_indices.get(0).unwrap();
            let key_field_type = key_field_types.get(0).unwrap();

            let from_body =
                generate_unnamed_one_tuple_from_body(key_field_index, &value_field_indices);
            generate_unnamed_one_tuple_impl_block(
                ident,
                key_field_index,
                key_field_type,
                &value_field_indices,
                value_field_types,
                from_body,
            )
        }
        _ => {
            let from_body =
                generate_unnamed_struct_from_body(&key_field_indices, &value_field_indices);
            generate_unnamed_impl_block(
                ident,
                &key_field_indices,
                key_field_types,
                &value_field_indices,
                value_field_types,
                from_body,
            )
        }
    }
}

pub fn derive(input: TokenStream) -> TokenStream {
    let DeriveInput { ident, data, .. } = parse_macro_input!(input);

    match data.clone() {
        syn::Data::Struct(data) => match data.clone().fields {
            syn::Fields::Named(_) => {
                return derive_named_struct(data, ident);
            }
            syn::Fields::Unnamed(_) => {
                return derive_unnamed_struct(data, ident);
            }
            syn::Fields::Unit => {
                todo!("Unit structs are not supported")
            }
        },
        _ => todo!("Currently only structs are supported"),
    };
}
