use crate::utils::Types;
use darling::{ast, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::*;

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
}

pub fn const_ident(field: TokenStream) -> TokenStream {
    let ident = parse_macro_input!(field as Ident);
    let field_id = const_field_id(&ident);

    field_id.into()
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    ChildInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
