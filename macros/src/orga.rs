use std::collections::HashMap;

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::ParseStream, *};

pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);
    let mut item = parse_macro_input!(input as ItemStruct);
    let root_attrs = item.attrs.clone();

    let name = item.ident.clone();
    let opts = StructOpts::from_list(&attr_args).unwrap();

    let mut fields = vec![];
    for field in item.fields.iter_mut() {
        let name = field.ident.clone().expect("named fields only");
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
    let latest_variant_sfx = format_ident!("V{}", opts.version);
    let latest_variant_name = format_ident!("{}V{}", name, opts.version);

    let specific_variant_attributes = {
        let mut specific_variant_attributes = (1..=opts.version)
            .into_iter()
            .map(|v| {
                let sfx = format_ident!("V{}", v);
                let prev_name = format!("{}V{}", name, v - 1);
                quote! { #sfx(state(version = #v, previous = #prev_name))}
            })
            .collect::<Vec<_>>();
        // let v = opts.version;
        // let prev_name = format!("{}V{}", name, v - 1);
        // specific_variant_attributes.push(quote! { #latest_variant_sfx(#(#root_attrs),* state(version = #v, previous = #prev_name), derive(::orga::call::Call, ::orga::query::Query)) });
        // specific_variant_attributes.push(quote! { #latest_variant_sfx(#(#root_attrs),* state(version = #v, previous = #prev_name), derive(::orga::call::Call, ::orga::query::Query)) });
        specific_variant_attributes
    };

    let ss_output = quote! {
        #[::superstruct::superstruct(
            variants(#(#sfxs),*),
            variant_attributes(derive(::orga::state::state2::State, ::orga::migrate::MigrateFrom, ::orga::encoding::VersionedDecode, ::orga::encoding::VersionedEncode, Default), #(#root_attrs),*),
            specific_variant_attributes(#(#specific_variant_attributes),*),
            no_enum
        )]
        #item
        type #name = #latest_variant_name;
    };

    quote! {
     #ss_output
    }
    .into()
}

#[derive(FromMeta, Debug)]
struct StructOpts {
    version: u8,
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
