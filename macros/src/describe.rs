use super::utils::gen_param_input;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use std::str::FromStr;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let num_to_token = |n: usize| TokenStream2::from_str(&n.to_string()).unwrap();
    let names = struct_fields(&item).enumerate().map(|(i, field)| {
        field
            .ident
            .clone()
            .map(|name| name.into_token_stream())
            .unwrap_or_else(|| num_to_token(i))
    });
    let types = struct_fields(&item).map(|field| &field.ty);
    let types_where = struct_fields(&item).map(|field| &field.ty);

    let name = &item.ident;
    let mut generics = item.generics.clone();
    generics.params.iter_mut().for_each(|p| {
        if let GenericParam::Type(tp) = p {
            tp.default.take();
        }
    });
    let where_clause = generics
        .where_clause
        .clone()
        .unwrap_or(parse_quote!(where))
        .predicates;
    let generic_params = gen_param_input(&generics, true);

    let output = quote! {
        impl #generics ::orga::describe::Describe for #name #generic_params
        where
            Self: ::orga::state::State + 'static,
            #(#types_where: ::orga::state::State + ::orga::describe::Describe + 'static,)*
            #where_clause
        {
            fn describe() -> ::orga::describe::Descriptor {
                ::orga::describe::Builder::new::<Self>().meta::<u8>()
                #(
                    .named_child_from_state::<Self, #types>(
                        stringify!(#names),
                    )
                )*
                .build()
            }
        }
    };

    output.into()
}

fn struct_fields(item: &DeriveInput) -> impl Iterator<Item = &Field> {
    let data = match item.data {
        Data::Struct(ref data) => data,
        Data::Enum(ref _data) => todo!("#[derive(Describe)] does not yet support enums"),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    match data.fields {
        Fields::Named(ref fields) => fields.named.iter(),
        Fields::Unnamed(ref fields) => fields.unnamed.iter(),
        Fields::Unit => panic!("Unit structs are not supported"),
    }
}
