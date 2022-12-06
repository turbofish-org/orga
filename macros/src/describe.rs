use super::utils::gen_param_input;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::str::FromStr;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let names = struct_fields(&item).map(|field| &field.ident);
    let types = struct_fields(&item).map(|field| &field.ty);
    let types_where = struct_fields(&item).map(|field| &field.ty);
    let indexes =
        (0..struct_fields(&item).count()).map(|i| TokenStream2::from_str(&i.to_string()).unwrap());

    let name = &item.ident;
    let generics = &item.generics;
    let where_clause = generics
        .where_clause
        .clone()
        .unwrap_or(parse_quote!(where))
        .predicates;
    let generic_params = gen_param_input(&generics, true);

    let output = quote! {
        impl#generics ::orga::describe::Describe for #name#generic_params
        where
            #(#types_where: ::orga::describe::Describe + 'static,)*
            #where_clause
        {
            fn describe() -> ::orga::describe::Descriptor {
                ::orga::describe::Builder::new::<Self>()
                #(
                    .named_child::<#types>(
                        stringify!(#names),
                        &[#indexes],
                        |v| ::orga::describe::Builder::access(v, |v: Self| v.#names)
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
