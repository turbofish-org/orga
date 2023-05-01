use crate::utils::Types;
use darling::{ast, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse::Parse, punctuated::Punctuated, *};

#[derive(Clone, Debug, FromDeriveInput)]
#[darling(supports(struct_named))]
pub struct ChildInputReceiver {
    ident: Ident,
    generics: Generics,
    data: ast::Data<(), ChildFieldReceiver>,
}

#[derive(Clone, Debug, FromField)]
pub struct ChildFieldReceiver {
    ident: Option<Ident>,
    ty: Type,
}

impl ToTokens for ChildInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Types {
            child_field_ty,
            child_trait,
            ..
        } = Types::default();

        let Self {
            ident,
            generics,
            data,
        } = &self;

        let (imp, ty, wher) = generics.split_for_impl();
        let fields = data.as_ref().take_struct().unwrap().fields.clone();

        for field in fields {
            let field_ident = field.ident.as_ref().unwrap();
            let field_ty = &field.ty;
            let const_field_id = const_field_id(field_ident);

            tokens.extend(quote! {
                impl #imp #child_trait for #child_field_ty < #ident #ty, #const_field_id >   #wher {
                    type Child = #field_ty;
                }
            });
        }
    }
}

pub fn const_field_id(ident: &Ident) -> TokenStream2 {
    let ident = ident.to_string();
    return quote! { #ident };
    // let ident = ident.as_bytes();
    // let ident = ident.iter().map(|b| *b as u128).collect_vec();

    // let mut id = 0;
    // for (i, b) in ident.iter().enumerate() {
    //     id += b << (i * 8);
    //     if i == 15 {
    //         break;
    //     }
    // }

    // quote! { #id }
}

pub fn const_ident(field: TokenStream) -> TokenStream {
    let ident = parse_macro_input!(field as Ident);
    let field_id = const_field_id(&ident);

    field_id.into()
}

struct ResolveInput {
    root_type: Path,
    segments: Punctuated<Ident, Token![.]>,
}

impl Parse for ResolveInput {
    fn parse(input: parse::ParseStream) -> Result<Self> {
        let root_type = input.parse()?;
        let _ = input.parse::<Token![,]>()?;
        let segments = Punctuated::parse_terminated(input)?;
        Ok(Self {
            root_type,
            segments,
        })
    }
}
pub fn resolve_path(input: TokenStream) -> TokenStream {
    unimplemented!()
    // let Types {
    //     child_trait,
    //     child_field_ty,
    //     ..
    // } = Types::default();
    // let inp = input.clone();
    // let item = parse_macro_input!(inp as ResolveInput);
    // let root_type = item.root_type;

    // let def_site = Span::def_site();
    // let resolved_type = item
    //     .segments
    //     .iter()
    //     .skip(1)
    //     .fold(quote_spanned! { def_site=> #root_type }, |acc, segment| {
    //         let const_field_id = const_field_id(segment);
    //         quote_spanned! { def_site=> <#child_field_ty<#acc, #const_field_id> as #child_trait>::Child}
    //     });

    // let call_site = Span::call_site();
    // let output = quote_spanned! { call_site=>
    //     #resolved_type
    // };

    // output.into()
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    ChildInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
