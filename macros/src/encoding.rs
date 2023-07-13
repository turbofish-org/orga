use darling::{ast, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::*;

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(encoding), supports(struct_any))]
pub struct EncodingInputReceiver {
    ident: Ident,
    generics: Generics,
    data: ast::Data<(), EncodingFieldReceiver>,

    #[darling(default)]
    pub version: u8,
    pub previous: Option<Path>,
    pub as_type: Option<Path>,
}

impl ToTokens for EncodingInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let EncodingInputReceiver {
            ident,
            generics,
            data,
            version,
            previous,
            as_type,
        } = self;
        let encode_trait = quote! { ::orga::encoding::Encode };
        let decode_trait = quote! { ::orga::encoding::Decode };
        let terminated_trait = quote! { ::orga::encoding::Terminated };
        let encoder_ty = quote! { ::orga::encoding::encoder::Encoder };
        let decoder_ty = quote! { ::orga::encoding::decoder::Decoder };
        let result_ty = quote! { ::orga::encoding::Result };

        let (imp, ty, wher) = generics.split_for_impl();
        let wher = if wher.is_some() {
            quote! { #wher }
        } else {
            quote! { where }
        };

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

        let encoding_len_method = if let Some(as_type) = as_type {
            quote! {
                fn encoding_length(&self) -> #result_ty<usize> {
                    #encoder_ty::<Vec<u8>>::encoding_length_as::<#as_type, &Self>(self)
                }
            }
        } else {
            let child_encoding_lens = field_names().map(|name| {
                quote! { + #encode_trait::encoding_length(&self.#name)? }
            });
            quote! {
                fn encoding_length(&self) -> #result_ty<usize> {
                    Ok(1 #(#child_encoding_lens)*)
                }
            }
        };

        let encode_into_method = if let Some(as_type) = as_type {
            quote! {
                fn encode_into<__W: ::std::io::Write>(&self, out: &mut __W) -> #result_ty<()> {
                    #encoder_ty::new(out).version(#version)?.encode_child_as::<#as_type, _>(self)?;
                    Ok(())
                }
            }
        } else {
            let child_encodes = fields_with_names().map(|(field, name)| match field.as_type {
                Some(ref as_type) => quote! {.encode_child_as::<#as_type, _>(&self.#name)?},
                None => quote! {.encode_child(&self.#name)?},
            });

            quote! {
                fn encode_into<__W: ::std::io::Write>(&self, out: &mut __W) -> #result_ty<()> {
                    #encoder_ty::new(out).version(#version)?
                    #(#child_encodes)*;

                    Ok(())
                }
            }
        };

        let decode_method = {
            let decode_value = match as_type {
                Some(as_type) => {
                    quote! { decoder.decode_child_as::<#as_type, _>()?}
                }
                None => {
                    let child_self_decodes =
                        fields_with_names().map(|(field, name)| match field.as_type {
                            Some(ref as_type) => {
                                quote! { #name: decoder.decode_child_as::<#as_type, _>()? }
                            }
                            None => quote! { #name: decoder.decode_child()? },
                        });
                    quote! { Self { #(#child_self_decodes),* } }
                }
            };

            quote! {
                fn decode<__R: ::std::io::Read>(mut input: __R) -> #result_ty<Self> {
                    let mut decoder = #decoder_ty::new(input, #version);
                    let mut value = #decode_value;

                    Ok(value)
                }
            }
        };

        let encode_where = {
            let field_encode_bounds = if let Some(ref as_type) = self.as_type {
                vec![quote! { #as_type: #encode_trait }]
            } else {
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, field)| {
                        let ty = &field.ty;
                        let maybe_term = if i < fields.len() - 1 {
                            quote! { + #terminated_trait }
                        } else {
                            quote! {}
                        };
                        quote! {
                            #ty: #encode_trait #maybe_term
                        }
                    })
                    .collect()
            };

            let bounds = quote! {
                #(#field_encode_bounds),*
            };

            quote! { #wher #bounds }
        };

        let decode_where = {
            let field_decode_bounds = if let Some(ref as_type) = self.as_type {
                vec![quote! { #as_type: #encode_trait }]
            } else {
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, field)| {
                        let ty = &field.ty;
                        let maybe_term = if i < fields.len() - 1 {
                            quote! { + #terminated_trait }
                        } else {
                            quote! {}
                        };
                        quote! {
                            #ty: #decode_trait #maybe_term
                        }
                    })
                    .collect()
            };

            let bounds = quote! {
                #(#field_decode_bounds),*
            };

            quote! { #wher #bounds }
        };

        let term_where = {
            let maybe_prev_term = previous
                .as_ref()
                .map(|prev| quote! { #prev: #terminated_trait, })
                .unwrap_or_default();

            let field_term_bounds = if let Some(ref as_type) = self.as_type {
                vec![quote! { #as_type: #terminated_trait }]
            } else {
                fields_with_names()
                    .map(|(field, _)| {
                        let ty = &field.ty;
                        quote! {
                            #ty: #terminated_trait
                        }
                    })
                    .collect()
            };

            let bounds = quote! {
                #maybe_prev_term
                #(#field_term_bounds),*
            };

            quote! { #wher #bounds }
        };

        tokens.extend(quote! {
            impl #imp #encode_trait for #ident #ty #encode_where {
                #encode_into_method

                #encoding_len_method
            }

            impl #imp #decode_trait for #ident #ty #decode_where {
                #decode_method
            }

            impl #imp #terminated_trait for #ident #ty #term_where {}

        });
    }
}

#[derive(Debug, FromField)]
#[darling(attributes(encoding))]
struct EncodingFieldReceiver {
    ty: Type,
    ident: Option<Ident>,
    as_type: Option<Ident>,
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    EncodingInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
