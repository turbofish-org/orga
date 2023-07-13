use darling::{
    ast,
    usage::{GenericsExt, Options, Purpose, UsesTypeParams},
    uses_lifetimes, uses_type_params, FromDeriveInput, FromField,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::*;

use crate::utils::{generics_union, replace_type_segment};

#[derive(FromDeriveInput)]
#[darling(attributes(migrate_from), supports(struct_any))]
struct MigrateFromInputReceiver {
    ident: Ident,
    generics: Generics,
    data: ast::Data<(), MigrateFromFieldReceiver>,

    #[darling(default)]
    identity: bool,
}

impl ToTokens for MigrateFromInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let MigrateFromInputReceiver {
            ident,
            generics,
            data,
            identity,
        } = self;

        let suffix_generics = |generics: &Generics, sfx| {
            let mut suffixed = generics.clone();
            suffixed.type_params_mut().for_each(|tp| {
                tp.ident = format_ident!("{}{}", tp.ident, sfx);
            });
            let search_options: Options = Purpose::BoundImpl.into();
            let decl_tp = generics.declared_type_params();
            if let Some(wher) = suffixed.where_clause.as_mut() {
                wher.predicates.iter_mut().for_each(|p| {
                    if let WherePredicate::Type(ty) = p {
                        let usages = ty.uses_type_params_cloned(&search_options, &decl_tp);
                        for usage in usages.iter() {
                            replace_type_segment(
                                &mut ty.bounded_ty,
                                &usage,
                                &format_ident!("{}{}", usage, sfx),
                            );
                        }
                    }
                });
            }

            suffixed
        };
        let old_generics = suffix_generics(generics, "1");
        let new_generics = suffix_generics(generics, "2");
        let (_, ty1, _) = old_generics.split_for_impl();
        let (_, ty2, _) = new_generics.split_for_impl();
        let union_generics = generics_union(&old_generics, &new_generics);
        let (imp_union, _, wher_union) = union_generics.split_for_impl();

        if *identity {
            let (imp, ty, wher) = old_generics.split_for_impl();
            return tokens.extend(quote! {
                impl #imp ::orga::migrate::MigrateFrom for #ident #ty
                #wher
                {
                    fn migrate_from(other: Self) -> ::orga::Result<Self> {
                        Ok(other)
                    }
                }
            });
        }

        let fields = data.as_ref().take_struct().unwrap().fields;

        let field_migrations = fields.iter().enumerate().map(|(i, f)| {
            let field_ident = f.ident.as_ref().map(|v| quote!(#v)).unwrap_or_else(|| {
                let i = syn::Index::from(i);
                quote!(#i)
            });

            quote! { #field_ident: ::orga::migrate::MigrateInto::migrate_into(other.#field_ident)?,}
        });

        let search_options: Options = Purpose::BoundImpl.into();
        let decl_tp = generics.declared_type_params();

        let bounds = fields.iter().filter_map(|f| {
            let ty = &f.ty;
            let usages = ty.uses_type_params_cloned(&search_options, &decl_tp);
            let mut old_ty = ty.clone();
            let mut new_ty = ty.clone();
            for usage in usages.iter() {
                replace_type_segment(&mut old_ty, &usage, &format_ident!("{}1", usage));
                replace_type_segment(&mut new_ty, &usage, &format_ident!("{}2", usage));
            }
            if usages.is_empty() {
                None
            } else {
                Some(quote! { #new_ty: ::orga::migrate::MigrateFrom<#old_ty>, })
            }
        });
        let wher = match wher_union {
            Some(wher) => quote! { #wher, #(#bounds)*},
            None => quote! { where #(#bounds)* },
        };

        tokens.extend(quote! {
            impl #imp_union ::orga::migrate::MigrateFrom<#ident #ty1> for #ident #ty2 #wher
            {
                fn migrate_from(other: #ident #ty1) -> ::orga::Result<Self> {
                    Ok(Self {
                        #(#field_migrations)*
                    })
                }
            }
        })
    }
}

#[derive(FromField)]
struct MigrateFromFieldReceiver {
    ident: Option<Ident>,
    ty: Type,
}

uses_type_params!(MigrateFromFieldReceiver, ty);
uses_lifetimes!(MigrateFromFieldReceiver, ty);

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    MigrateFromInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
