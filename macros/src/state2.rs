use super::utils::gen_param_input;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::{collections::HashMap, str::FromStr};
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let mut is_tuple_struct = false;
    match &item.data {
        Data::Struct(data) => {
            if let Fields::Unnamed(_) = data.fields {
                is_tuple_struct = true
            }
        }
        _ => todo!("Currently only structs are supported"),
    }

    let field_names = || struct_fields(&item).map(|field| &field.ident);
    let seq =
        || (0..field_names().count()).map(|i| TokenStream2::from_str(&i.to_string()).unwrap());

    let name = &item.ident;
    let generics = &item.generics;
    let where_clause = &generics.where_clause;
    let generic_params = gen_param_input(&generics, true);

    let StateInfo { previous, version } = parse_state_info(&item);

    let seq_substore = seq();
    let attach_body = if is_tuple_struct {
        let seq_field = seq();
        quote!(
            // #(::orga::state::State2::<::orga::store::DefaultBackingStore>::attach(&mut self.#seq_field, store.sub(&[#seq_substore]))?;)*
            Ok(())
        )
    } else {
        let names = field_names();
        quote!(
            ::orga::state::Attacher::new(store)
                #(.attach_child(&mut self.#names)?)*;
            // #(::orga::state::State::<::orga::store::DefaultBackingStore>::attach(&mut self.#names, store.sub(&[#seq_substore]))?;)*
            Ok(())
        )
    };

    let flush_body = if is_tuple_struct {
        let indexes = seq();
        quote!(
            // #(::orga::state::State::<::orga::store::DefaultBackingStore>::flush(&mut self.#indexes)?;)*
            Ok(())
        )
    } else {
        let names = field_names();
        quote!(
            ::orga::state::Flusher::new(out)
                .version(#version)?
                #(.flush_child(&mut self.#names)?)*;

            Ok(())
        )
    };

    let load_body_inner = if is_tuple_struct {
        let indexes = seq();
        quote!(
            // #(::orga::state::State::<::orga::store::DefaultBackingStore>::flush(&mut self.#indexes)?;)*
            Ok(())
        )
    } else {
        let names = field_names();
        quote! {
            Self {
                #(#names: loader.load_child()?,)*
            }
        }
    };

    let load_value = if let Some(previous) = previous {
        quote! {
            if let Some(prev) = loader.maybe_load_from_prev::<#previous, _>()? {
                prev
            } else {
                #load_body_inner
            }
        }
    } else {
        quote! {
            #load_body_inner
        }
    };

    let load_body = {
        quote!(
            let mut loader = ::orga::state::Loader::new(store.clone(), bytes, #version);

            let mut value = #load_value;

            value.attach(store)?;

            Ok(value)
        )
    };

    let output = quote! {
        impl#generics ::orga::state::state2::State for #name#generic_params
        #where_clause
        {
            fn attach(
                &mut self,
                store: ::orga::store::Store,
            ) -> ::orga::Result<()> {
                #attach_body
            }

            fn flush<__ORGA_WRITE: ::std::io::Write>(
                &mut self, out: &mut __ORGA_WRITE
            ) -> ::orga::Result<()> {
                #flush_body
            }

            fn load(store: ::orga::store::Store, bytes: &mut &[u8]) -> ::orga::Result<Self> {
                #load_body
            }

        }
    };

    output.into()
}

fn struct_fields(item: &DeriveInput) -> impl Iterator<Item = &Field> {
    let data = match item.data {
        Data::Struct(ref data) => data,
        Data::Enum(ref _data) => todo!("#[derive(State)] does not yet support enums"),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    match data.fields {
        Fields::Named(ref fields) => fields.named.iter(),
        Fields::Unnamed(ref fields) => fields.unnamed.iter(),
        Fields::Unit => panic!("Unit structs are not supported"),
    }
}

pub struct StateInfo {
    pub version: u8,
    pub previous: Option<syn::Ident>,
}
pub fn parse_state_info(item: &DeriveInput) -> StateInfo {
    let attr = item
        .attrs
        .iter()
        .find(|attr| attr.path.is_ident("state"))
        .map(|attr| {
            let mut map = HashMap::new();
            let args = attr.parse_meta().unwrap();
            if let syn::Meta::List(list) = args {
                for entry in list.nested {
                    if let syn::NestedMeta::Meta(syn::Meta::NameValue(entry)) = entry {
                        let key = entry.path.get_ident().unwrap().to_string();
                        map.insert(key, entry.lit);
                    } else {
                        panic!()
                    }
                }
            } else {
                panic!()
            }

            map
        });

    let version = attr.as_ref().map_or(0, |attr| {
        attr.get("version").map_or(0, |value| {
            if let syn::Lit::Int(int) = value {
                let v: u8 = int.base10_parse().unwrap();
                v
            } else {
                panic!()
            }
        })
    });

    let previous = attr.as_ref().map_or(None, |attr| {
        attr.get("previous").map_or(None, |value| {
            if let syn::Lit::Str(prev_name) = value {
                let prev_name =
                    syn::Ident::new(prev_name.value().as_str(), proc_macro2::Span::call_site());

                Some(prev_name)
            } else {
                panic!()
            }
        })
    });

    StateInfo { version, previous }
}
