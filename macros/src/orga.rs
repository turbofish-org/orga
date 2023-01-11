use std::collections::HashMap;

use super::utils::gen_param_input;
use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::*;

pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);
    let mut item = parse_macro_input!(input as ItemStruct);
    let root_attrs = item.attrs.clone();
    let vis = item.vis.clone();

    let generics = &item.generics;
    let generic_params = gen_param_input(&generics, true);

    let name = item.ident.clone();
    let opts = StructOpts::from_list(&attr_args).unwrap();

    let maybe_default = if opts.skip.is_some()
        && opts
            .skip
            .as_ref()
            .unwrap()
            .contains_key(&format_ident!("Default"))
    {
        quote! {}
    } else {
        quote! { Default, }
    };

    let maybe_call = if opts.skip.is_some()
        && opts
            .skip
            .as_ref()
            .unwrap()
            .contains_key(&format_ident!("Call"))
    {
        quote! {}
    } else {
        quote! { ::orga::call::Call, }
    };

    let maybe_query = if opts.skip.is_some()
        && opts
            .skip
            .as_ref()
            .unwrap()
            .contains_key(&format_ident!("Query"))
    {
        quote! {}
    } else {
        quote! { ::orga::query::Query, }
    };

    let maybe_state = if opts.skip.is_some()
        && opts
            .skip
            .as_ref()
            .unwrap()
            .contains_key(&format_ident!("State"))
    {
        quote! {}
    } else {
        quote! { ::orga::state::State, }
    };

    let maybe_migrate_from = if opts.skip.is_some()
        && opts
            .skip
            .as_ref()
            .unwrap()
            .contains_key(&format_ident!("MigrateFrom"))
    {
        quote! {}
    } else {
        quote! { ::orga::migrate::MigrateFrom, }
    };

    let mut fields = vec![];
    for field in item.fields.iter_mut() {
        let name = field
            .ident
            .clone()
            .expect("Only named fields are supported");

        let field_opts = field
            .attrs
            .iter()
            .filter(|attr| is_orga_attr(attr))
            .find_map(|attr| {
                let meta = attr.parse_meta().unwrap();
                let opts = FieldOpts::from_meta(&meta).unwrap();

                Some(opts)
            })
            .unwrap_or_default();

        field.attrs = discard_orga_attrs(&field.attrs);

        let version: Option<Vec<Ident>> = field_opts.version.map(|v| v.keys().cloned().collect());
        if let Some(version) = version.clone() {
            field.attrs.push(parse_quote! {
                #[superstruct(only(#(#version),*))]
            });
        }

        fields.push(FieldData {
            name,
            field: field.clone(),
            version,
        });
    }

    let root_attrs = root_attrs.into_iter().map(|attr| {
        let path = &attr.path;
        let tokens = &attr.tokens;

        quote! {
           #path#tokens,
        }
    });

    let sfxs = opts.version_suffixes();
    let latest_variant_name = format_ident!("{}V{}", name, opts.version);

    let specific_variant_attributes = (0..=opts.version)
        .into_iter()
        .map(|v| {
            let prev = if v > 0 {
                let prev_name = format!("{}V{}", name, v - 1);
                quote! { previous = #prev_name }
            } else {
                quote! {}
            };

            let derive_attr = if v == opts.version {
                quote! { derive(#maybe_call #maybe_query) }
            } else {
                quote! {}
            };

            let sfx = format_ident!("V{}", v);
            quote! { #sfx(state(version = #v, #prev), #derive_attr)}
        })
        .collect::<Vec<_>>();

    let ss_output = quote! {
        #[::orga::superstruct(
            variants(#(#sfxs),*),
            variant_attributes(derive(#maybe_state #maybe_migrate_from ::orga::encoding::VersionedDecode, ::orga::encoding::VersionedEncode, #maybe_default), #(#root_attrs),*),
            specific_variant_attributes(#(#specific_variant_attributes),*),
            no_enum
        )]
        #item
        #vis type #name#generic_params = #latest_variant_name#generic_params;
    };

    quote! {
     #ss_output
    }
    .into()
}

#[derive(FromMeta, Debug)]
struct StructOpts {
    #[darling(default)]
    version: u8,
    skip: Option<HashMap<Ident, ()>>,
}

impl StructOpts {
    fn version_suffixes(&self) -> impl Iterator<Item = Ident> {
        (0..=self.version).map(variant_ident)
    }
}

#[derive(Debug, FromMeta, Default)]
struct FieldOpts {
    version: Option<HashMap<Ident, ()>>,
}

#[derive(Debug)]
struct FieldData {
    name: Ident,
    field: Field,
    version: Option<Vec<Ident>>,
}

fn variant_ident(v: u8) -> Ident {
    format_ident!("V{}", v)
}

fn is_orga_attr(attr: &Attribute) -> bool {
    is_attr_with_ident(attr, "orga")
}

fn is_attr_with_ident(attr: &Attribute, ident: &str) -> bool {
    attr.path
        .get_ident()
        .map_or(false, |attr_ident| attr_ident.to_string() == ident)
}

fn discard_orga_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| !is_orga_attr(attr))
        .cloned()
        .collect()
}
