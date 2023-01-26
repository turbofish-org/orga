use super::utils::{named_fields, Types};
use darling::{
    ast,
    usage::{GenericsExt, Options, Purpose, UsesTypeParams},
    uses_type_params, FromDeriveInput, FromField,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::*;

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(state), supports(struct_any))]
pub struct StateInputReceiver {
    ident: Ident,
    generics: syn::Generics,
    data: ast::Data<(), StateFieldReceiver>,

    #[darling(default)]
    pub version: u8,
    pub previous: Option<Ident>,
    pub as_type: Option<Path>,
    #[darling(default)]
    transparent: bool,
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
                    } else {
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

        let Self {
            version, previous, ..
        } = self;

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
                        quote! { #name: loader.load_child()? }
                    }
                }
            });
            quote! { Self { #(#child_self_loads),* } }
        };

        let load_value = if let Some(previous) = previous {
            quote! {
                if let Some(prev) = loader.maybe_load_from_prev::<#previous, _>()? {
                    prev
                } else {
                    #load_value
                }
            }
        } else {
            quote! {
                #load_value
            }
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

    fn bounds(&self) -> TokenStream2 {
        let Types {
            terminated_trait,
            state_trait,
            ..
        } = Default::default();
        let n_fields = self.state_fields().len();
        self.state_fields()
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
            .collect()
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
            }
        });
    }
}

#[derive(Debug, FromField, Clone)]
#[darling(attributes(state))]
struct StateFieldReceiver {
    ident: Option<Ident>,
    ty: Type,

    as_type: Option<Ident>,
    #[darling(default)]
    skip: bool,
    #[darling(default)]
    transparent: bool,
}
uses_type_params!(StateFieldReceiver, ty);

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    StateInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
