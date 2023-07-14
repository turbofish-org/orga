use std::collections::HashSet;

use super::{
    state,
    utils::{named_fields, Types},
};
use darling::{
    ast,
    export::NestedMeta,
    usage::{GenericsExt, Options, Purpose, UsesTypeParams},
    uses_type_params, FromDeriveInput, FromField, FromMeta,
};
use itertools::Itertools;
use proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::*;

#[derive(Debug, FromDeriveInput, Clone)]
#[darling(attributes(state), supports(struct_any))]
pub struct MigrateInputReceiver {
    ident: Ident,
    generics: syn::Generics,
    data: ast::Data<(), state::StateFieldReceiver>,

    #[darling(default)]
    pub version: u8,
    #[darling(default)]
    transparent: bool,
}

impl MigrateInputReceiver {
    fn transparent_inner(&self) -> Option<(TokenStream2, state::StateFieldReceiver)> {
        let fields = self.data.as_ref().take_struct().unwrap().fields.clone();
        let state_fields = fields.iter().filter(|f| !f.skip).collect::<Vec<_>>();
        let n_marked_fields = fields
            .iter()
            .filter(|f| f.transparent)
            .collect::<Vec<_>>()
            .len();
        if n_marked_fields > 1 {
            panic!("Only one field can be marked as transparent")
        }
        if self.transparent && n_marked_fields == 0 && state_fields.len() > 1 {
            panic!("Transparent state struct must have exactly one field or have one field marked transparent")
        }
        if !self.transparent && n_marked_fields == 0 {
            return None;
        }
        if n_marked_fields == 1 {
            return named_fields!(self)
                .find(|(_, f)| f.transparent)
                .map(|(ident, field)| (ident, field.clone()));
        }
        if self.transparent && state_fields.len() == 1 {
            return named_fields!(self)
                .find(|(_, f)| !f.skip)
                .map(|(ident, field)| (ident, field.clone()));
        }
        unreachable!()
    }

    fn migrate_method(&self) -> TokenStream2 {
        let Types {
            store_ty,
            result_ty,
            ..
        } = Default::default();

        let Self { version, .. } = self;

        let migrate = if let Some((inner_name, _field)) = self.transparent_inner() {
            let child_transparent_other_loads = named_fields!(self)
                .filter(|(name, _field)| name.to_string() != inner_name.to_string())
                .map(|(name, _field)| {
                    quote! { #name: migration.migrate_transparent_child_other()? }
                });
            quote! { Self {
                #inner_name: migration.migrate_transparent_child_inner()?,
                #(#child_transparent_other_loads),*
            }}
        } else {
            let child_self_loads = named_fields!(self).map(|(name, field)| match field.as_type {
                Some(ref as_type) => {
                    quote! { #name: migration.migrate_child_as::<#as_type, _>()? }
                }
                None => {
                    if field.skip {
                        quote! { #name: migration.migrate_skipped_child()? }
                    } else {
                        quote! { #name: migration.migrate_child::<Self, _>()? }
                    }
                }
            });
            quote! { Self { #(#child_self_loads),* } }
        };

        quote! {
            fn migrate(src: #store_ty, dest: #store_ty, bytes: &mut &[u8]) -> #result_ty<Self> {
                let mut migration = ::orga::migrate::Migration::new(src, dest.clone(), bytes, #version);
                let mut value: Self = #migrate;
                ::orga::state::State::attach(&mut value, dest)?;
                Ok(value)
            }
        }
    }

    fn bounds(&self) -> TokenStream2 {
        let Types {
            terminated_trait, ..
        } = Default::default();
        let n_fields = self.state_fields().len();
        let field_bounds: TokenStream2 = self
            .state_fields()
            .iter()
            .enumerate()
            .map(|(i, (_name, field))| {
                let field_ty = &field.ty;
                let maybe_term_bound = if i < n_fields - 1 {
                    quote! { #field_ty: #terminated_trait, }
                } else {
                    quote! {}
                };
                let opts: Options = Purpose::BoundImpl.into();
                let tys = self.generics.declared_type_params().into();
                let uses_generic = !field.uses_type_params(&opts, &tys).is_empty();
                let maybe_migrate_bound = if uses_generic {
                    quote! { #field_ty: ::orga::migrate::Migrate, }
                } else {
                    quote! {}
                };
                quote! { #maybe_term_bound #maybe_migrate_bound }
            })
            .collect();

        quote! { Self: 'static, #field_bounds }
    }

    fn state_fields(&self) -> Vec<(TokenStream2, state::StateFieldReceiver)> {
        if let Some(inner) = self.transparent_inner() {
            vec![inner]
        } else {
            named_fields!(self)
                .filter(|(_name, field)| !field.skip)
                .map(|(name, field)| (name, field.clone()))
                .collect()
        }
    }
}

impl ToTokens for MigrateInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let MigrateInputReceiver {
            ident, generics, ..
        } = self;

        let (imp, ty, wher) = generics.split_for_impl();
        let migrate_method = self.migrate_method();
        let bounds = self.bounds();

        let wher = if wher.is_some() {
            quote! { #wher #bounds  }
        } else {
            quote! { where #bounds  }
        };

        tokens.extend(quote! {
            impl #imp ::orga::migrate::Migrate for #ident #ty #wher {
                #migrate_method
            }
        });
    }
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    MigrateInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
