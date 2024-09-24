use std::collections::HashSet;

use super::utils::{named_fields, Types};
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
#[darling(
    attributes(state),
    supports(struct_any),
    and_then = "StateInputReceiver::ensure_prefixes"
)]
pub struct StateInputReceiver {
    pub ident: Ident,
    pub generics: syn::Generics,
    pub data: ast::Data<(), StateFieldReceiver>,

    #[darling(default)]
    pub version: u8,
    #[darling(default)]
    #[allow(dead_code)]
    pub previous: Option<Path>,
    #[darling(default)]
    pub as_type: Option<Path>,
    #[darling(default)]
    pub transparent: bool,
    #[darling(default)]
    pub allow_prefix_overlap: bool,
}

impl StateInputReceiver {
    fn transparent_inner(&self) -> Option<(TokenStream2, StateFieldReceiver)> {
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

    fn attach_method(&self) -> TokenStream2 {
        let Types {
            attacher_ty,
            result_ty,
            store_ty,
            ..
        } = Default::default();

        if let Some((name, _field)) = self.transparent_inner() {
            quote! {
                fn attach(&mut self, store: #store_ty) -> #result_ty<()> {
                    #attacher_ty::new(store).attach_transparent_child(&mut self.#name)?;
                    Ok(())
                }
            }
        } else if let Some(ref as_type) = self.as_type {
            quote! {
                fn attach(&mut self, store: #store_ty) -> #result_ty<()> {
                    #attacher_ty::new(store).attach_child_as::<#as_type, _>(self)?;
                    Ok(())
                }
            }
        } else {
            let child_attaches = named_fields!(self).map(|(name, field)| match field.as_type {
                Some(ref as_type) => quote! {.attach_child_as::<#as_type, _>(&mut self.#name)?},
                None => {
                    if field.skip {
                        quote! { .attach_skipped_child(&mut self.#name)?}
                    } else if let Some(ref pfx) = field.prefix() {
                        match pfx {
                            Prefix::Relative(bytes) => {
                                let byte_seq = bytes.iter().map(|b| quote! {#b});
                                let prefix = quote! {&[#(#byte_seq),*]};
                                quote! {.attach_child_with_relative_prefix(&mut self.#name, #prefix)?}
                            }
                            Prefix::Absolute(bytes) => {
                                let byte_seq = bytes.iter().map(|b| quote! {#b});
                                let prefix = quote! {vec![#(#byte_seq),*]};
                                quote! {.attach_child_with_absolute_prefix(&mut self.#name, #prefix)?}
                            },
                        }
                    }
                    else {
                        quote! {.attach_child(&mut self.#name)?}
                    }
                }
            });

            quote! {
                fn attach(&mut self, store: #store_ty) -> #result_ty<()> {
                    #attacher_ty::new(store)
                    #(#child_attaches)*;

                    Ok(())
                }
            }
        }
    }

    fn flush_method(&self) -> TokenStream2 {
        let Types {
            flusher_ty,
            result_ty,
            ..
        } = Default::default();
        let Self { version, .. } = self;

        if let Some((name, _field)) = self.transparent_inner() {
            quote! {
                fn flush<__W: ::std::io::Write>(self, out: &mut __W) -> #result_ty<()> {
                    #flusher_ty::new(out).version(#version)?.flush_transparent_child(self.#name)?;
                    Ok(())
                }
            }
        } else if let Some(ref as_type) = self.as_type {
            quote! {
                fn flush<__W: ::std::io::Write>(self, out: &mut __W) -> #result_ty<()> {
                    #flusher_ty::new(out).version(#version)?.flush_child_as::<#as_type, _>(self)?;
                    Ok(())
                }
            }
        } else {
            let child_flushes = named_fields!(self).map(|(name, field)| match field.as_type {
                Some(ref as_type) => quote! {.flush_child_as::<#as_type, _>(self.#name)?},
                None => {
                    if field.skip {
                        quote! { .flush_skipped_child(self.#name)?}
                    } else {
                        quote! {.flush_child(self.#name)?}
                    }
                }
            });

            quote! {
                fn flush<__W: ::std::io::Write>(self, out: &mut __W) -> #result_ty<()> {
                    #flusher_ty::new(out).version(#version)?
                    #(#child_flushes)*;

                    Ok(())
                }
            }
        }
    }

    fn load_method(&self) -> TokenStream2 {
        let Types {
            loader_ty,
            store_ty,
            result_ty,
            ..
        } = Default::default();

        let Self { version, .. } = self;

        let load_value = if let Some((inner_name, _field)) = self.transparent_inner() {
            let child_transparent_other_loads = named_fields!(self)
                .filter(|(name, _field)| name.to_string() != inner_name.to_string())
                .map(|(name, _field)| {
                    quote! { #name: loader.load_transparent_child_other()? }
                });
            quote! { Self {
                #inner_name: loader.load_transparent_child_inner()?,
                #(#child_transparent_other_loads),*
            }}
        } else if let Some(ref as_type) = self.as_type {
            quote! { loader.load_child_as::<#as_type, _>()?}
        } else {
            let child_self_loads = named_fields!(self).map(|(name, field)| match field.as_type {
                Some(ref as_type) => {
                    quote! { #name: loader.load_child_as::<#as_type, _>()? }
                }
                None => {
                    if field.skip {
                        quote! { #name: loader.load_skipped_child()? }
                    } else {
                        quote! { #name: loader.load_child::<Self, _>()? }
                    }
                }
            });
            quote! { Self { #(#child_self_loads),* } }
        };

        quote! {
            fn load(store: #store_ty, bytes: &mut &[u8]) -> #result_ty<Self> {
                let mut loader = #loader_ty::new(store.clone(), bytes, #version);
                let mut value: Self = #load_value;
                value.attach(store)?;

                Ok(value)
            }
        }
    }

    fn field_keyop_method(&self) -> TokenStream2 {
        let Types { keyop_ty, .. } = Default::default();
        let arms = self
            .state_fields()
            .iter()
            .map(|(name, field)| {
                let prefix = field.prefix();
                quote! {
                    stringify!(#name) => Some(#prefix),
                }
            })
            .collect_vec();

        quote! {
            fn field_keyop(field_name: &str) -> Option<#keyop_ty> {
                match field_name {
                    #(#arms)*
                    _ => None,
                }
            }
        }
    }

    fn bounds(&self) -> TokenStream2 {
        let Types {
            terminated_trait,
            state_trait,
            ..
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
                let maybe_state_bound = if uses_generic {
                    quote! { #field_ty: #state_trait, }
                } else {
                    quote! {}
                };
                quote! { #maybe_term_bound #maybe_state_bound }
            })
            .collect();

        quote! { Self: 'static, #field_bounds }
    }

    fn state_fields(&self) -> Vec<(TokenStream2, StateFieldReceiver)> {
        if let Some(inner) = self.transparent_inner() {
            vec![inner]
        } else {
            named_fields!(self)
                .filter(|(_name, field)| !field.skip)
                .map(|(name, field)| (name, field.clone()))
                .collect()
        }
    }

    fn ensure_prefixes(mut self) -> darling::Result<Self> {
        let mut prefixes: HashSet<Vec<u8>> = HashSet::new();
        let mut field_count = 0;
        self.data = self.data.clone().map_struct_fields(|field| {
            let mut field = field.clone();
            if matches!(field.prefix(), Some(Prefix::Absolute(_))) || field.skip {
                return field
            }
            let prefix = if let Some(ref prefix) = field.prefix {
                prefix.0.clone()
            } else if field.transparent {
                vec![]
            } else {
                vec![field_count]
            };
            field.prefix.replace(PrefixBytes(prefix.clone()));
            if !prefixes.insert(prefix) && !self.allow_prefix_overlap {
                panic!("Store prefix overlap detected. Consider adding `#[state(allow_prefix_overlap)]` to the parent if this is intended.");
            }

            field_count += 1;
            field
        });

        Ok(self)
    }
}

impl ToTokens for StateInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let StateInputReceiver {
            ident, generics, ..
        } = self;

        let Types { state_trait, .. } = Default::default();

        let (imp, ty, wher) = generics.split_for_impl();

        let attach_method = self.attach_method();
        let flush_method = self.flush_method();
        let load_method = self.load_method();
        let field_keyop_method = self.field_keyop_method();

        let bounds = self.bounds();

        let wher = if wher.is_some() {
            quote! { #wher #bounds  }
        } else {
            quote! { where #bounds  }
        };

        tokens.extend(quote! {
            impl #imp #state_trait for #ident #ty #wher {
                #attach_method
                #flush_method
                #load_method
                #field_keyop_method
            }
        });
    }
}

#[derive(Debug, FromField, Clone)]
#[darling(attributes(state))]
pub struct StateFieldReceiver {
    pub ident: Option<Ident>,
    pub ty: Type,

    #[darling(default)]
    pub as_type: Option<Ident>,
    #[darling(default)]
    pub skip: bool,
    #[darling(default)]
    pub transparent: bool,
    pub prefix: Option<PrefixBytes>,
    pub absolute_prefix: Option<PrefixBytes>,
}
uses_type_params!(StateFieldReceiver, ty);

impl StateFieldReceiver {
    fn prefix(&self) -> Option<Prefix> {
        match (self.prefix.as_ref(), self.absolute_prefix.as_ref()) {
            (Some(prefix_rel), None) => Some(Prefix::Relative(prefix_rel.0.clone())),
            (None, Some(prefix_abs)) => Some(Prefix::Absolute(prefix_abs.0.clone())),
            (None, None) => None,
            _ => panic!("cannot have both prefix and prefix_absolute"),
        }
    }
}

pub enum Prefix {
    Relative(Vec<u8>),
    Absolute(Vec<u8>),
}

impl ToTokens for Prefix {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        // This implementation expects the keyop to be used as a
        // describe::KeyOp. The store prefixing in `attach` parses the Prefix
        // enum directly instead of using the ToTokens trait.

        let Types { keyop_ty, .. } = Default::default();

        match self {
            Prefix::Relative(bytes) => {
                let byte_seq = bytes.iter().map(|b| quote! {#b});
                let prefix = quote! {vec![#(#byte_seq),*]};
                tokens.extend(quote! { #keyop_ty::Append(#prefix) })
            }
            Prefix::Absolute(bytes) => {
                let byte_seq = bytes.iter().map(|b| quote! {#b});
                let prefix = quote! {vec![#(#byte_seq),*]};
                tokens.extend(quote! { #keyop_ty::Absolute(#prefix) })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrefixBytes(Vec<u8>);

impl FromMeta for PrefixBytes {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let mut bytes = vec![];
        for item in items.iter() {
            match item {
                NestedMeta::Lit(Lit::Int(pfx)) => bytes.push(pfx.base10_parse().unwrap()),
                NestedMeta::Lit(Lit::ByteStr(pfx)) => bytes.extend(pfx.value()),
                _ => unimplemented!(),
            }
        }
        Ok(PrefixBytes(bytes))
    }
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    StateInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
