use darling::{ast, FromDeriveInput, FromField};
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
}

impl ToTokens for StateInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let StateInputReceiver {
            ident,
            generics,
            data,
            version,
            previous,
            as_type,
        } = self;
        let state_trait = quote! { ::orga::state::State };
        let store_ty = quote! { ::orga::store::Store };
        let attacher_ty = quote! { ::orga::state::Attacher };
        let flusher_ty = quote! { ::orga::state::Flusher };
        let loader_ty = quote! { ::orga::state::Loader };
        let result_ty = quote! { ::orga::Result };

        let (imp, ty, wher) = generics.split_for_impl();
        let struct_data = data.as_ref().take_struct().expect("Should never be enum");

        let fields = struct_data.fields.clone();

        let field_names = || {
            fields.iter().enumerate().map(|(i, f)| {
                f.ident.as_ref().map(|v| quote!(#v)).unwrap_or_else(|| {
                    let i = syn::Index::from(i);
                    quote!(#i)
                })
            })
        };

        let fields_with_names = || fields.iter().cloned().zip(field_names());

        let attach_method = if let Some(as_type) = as_type {
            quote! {
                fn attach(&mut self, store: #store_ty) -> #result_ty<()> {
                    #attacher_ty::new(store).attach_child_as::<#as_type, _>(self)?;
                    Ok(())
                }
            }
        } else {
            let child_attaches = fields_with_names().map(|(field, name)| match field.as_type {
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
        };

        let flush_method = if let Some(as_type) = as_type {
            quote! {
                fn flush<__W: ::std::io::Write>(self, out: &mut __W) -> #result_ty<()> {
                    #flusher_ty::new(out).version(#version)?.flush_child_as::<#as_type, _>(self)?;
                    Ok(())
                }
            }
        } else {
            let child_flushes = fields_with_names().map(|(field, name)| match field.as_type {
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
        };

        let load_method = {
            let load_value = match as_type {
                Some(as_type) => {
                    quote! { loader.load_child_as::<#as_type, _>()?}
                }
                None => {
                    let child_self_loads =
                        fields_with_names().map(|(field, name)| match field.as_type {
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
                }
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
                fn load(store: #store_ty, bytes: &mut &[u8]) -> ::orga::Result<Self> {
                    let mut loader = #loader_ty::new(store.clone(), bytes, #version);
                    let mut value: Self = #load_value;
                    value.attach(store)?;

                    Ok(value)
                }
            }
        };

        let bounds = {
            fields.iter().enumerate().map(|(i, field)| {
                let field_ty = &field.ty;
                let maybe_term_bound = if i < fields.len() - 1 {
                    quote! { #field_ty: ::orga::encoding::Terminated, }
                } else {
                    quote! {}
                };
                quote! { #maybe_term_bound }
            })
        };
        let wher = if wher.is_some() {
            quote! { #wher #(#bounds)*  }
        } else {
            quote! { where #(#bounds)*  }
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

#[derive(Debug, FromField)]
#[darling(attributes(state))]
struct StateFieldReceiver {
    ident: Option<Ident>,
    as_type: Option<Ident>,
    ty: Type,
    #[darling(default)]
    skip: bool,
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    StateInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
