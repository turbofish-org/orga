use std::collections::HashSet;

use darling::{
    ast,
    usage::{CollectTypeParams, GenericsExt, Purpose},
    uses_type_params, FromDeriveInput, FromField, ToTokens,
};
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::*;

use crate::{
    child::const_field_id,
    utils::{to_camel_case, to_snake_case, Types},
};

#[derive(Debug, Clone, FromDeriveInput)]
#[darling(supports(struct_named), forward_attrs)]
struct FieldCallInputReceiver {
    ident: Ident,
    generics: Generics,
    vis: Visibility,
    data: ast::Data<(), FieldCallFieldReceiver>,
}

impl FieldCallInputReceiver {
    fn call_fields(&self) -> Vec<FieldCallFieldReceiver> {
        self.data
            .as_ref()
            .take_struct()
            .unwrap()
            .fields
            .clone()
            .into_iter()
            .filter(|field| field.is_call())
            .cloned()
            .collect()
    }

    fn call_generics(&self) -> Generics {
        let Types { call_trait, .. } = Types::default();
        let mut ret_generics = self.generics.clone();
        let cf = self.call_fields();
        let params = cf.collect_type_params_cloned(
            &Purpose::Declare.into(),
            &self.generics.declared_type_params(),
        );
        ret_generics.params = ret_generics
            .params
            .into_iter()
            .filter(|gp| match gp {
                GenericParam::Type(ref ty) => params.contains(&ty.ident),
                _ => true,
            })
            .collect();

        let wher = ret_generics.make_where_clause();
        for field in cf.iter() {
            let ty = &field.ty;
            wher.predicates.push(parse_quote!(#ty: #call_trait))
        }
        ret_generics
    }

    fn unused_generics(&self) -> Generics {
        let call_generics = self.call_generics();
        let call_generics = call_generics.type_params().collect::<HashSet<_>>();

        let mut ret_generics = self.generics.clone();
        ret_generics.params = ret_generics
            .params
            .into_iter()
            .filter(|gp| match gp {
                GenericParam::Type(ref ty) => !call_generics.contains(ty),
                _ => true,
            })
            .collect();

        ret_generics
    }

    fn field_call_enum(&self) -> FieldCallEnum {
        FieldCallEnum {
            parent: self.clone(),
            vis: self.vis.clone(),
            ident: Ident::new(&format!("{}FieldCall", self.ident), Span::call_site()),
            data: self.call_fields(),
        }
    }

    fn call_bounds(&self) -> Vec<TokenStream2> {
        let Types { state_trait, .. } = Types::default();
        let mut bounds = self
            .call_fields()
            .into_iter()
            .map(|field| {
                let ty = &field.ty;
                let call_trait = quote! { ::orga::call::Call };
                quote! { #ty: #call_trait }
            })
            .collect_vec();

        let (_, ty, _) = self.generics.split_for_impl();
        let ident = &self.ident;

        bounds.push(quote! { #ident #ty: #state_trait});

        bounds
    }

    fn field_call_impl(&self, fc_enum: &FieldCallEnum) -> TokenStream2 {
        let Types {
            result_ty,
            field_call_trait,
            ..
        } = Types::default();
        let ident = &self.ident;
        let fc_enum_ident = &fc_enum.ident;
        let (imp, ty, wher) = self.generics.split_for_impl();
        let arms = fc_enum.data.iter().map(|v| {
            let cc_ident = to_camel_case(v.ident.as_ref().unwrap());
            let sc_ident = v.ident.as_ref().unwrap();
            quote! { #cc_ident(subcall) => ::orga::call::Call::call(&mut self.#sc_ident, subcall) }
        });

        let call_bounds = self.call_bounds();
        let wher = match wher {
            Some(w) => quote! { #w #(#call_bounds),* },
            None => quote! { where #(#call_bounds),* },
        };

        quote! {
            impl #imp #field_call_trait for #ident #ty #wher {
                type FieldCall = #fc_enum_ident #ty;
                fn field_call(&mut self, call: Self::FieldCall) -> #result_ty<()> {
                    use #fc_enum_ident::*;
                    match call {
                        #(#arms,)*
                        Noop(_) => Ok(()),
                    }
                }
            }
        }
    }

    fn call_builder(&self) -> TokenStream2 {
        let Types {
            build_call_trait,
            call_trait,
            field_call_trait,
            method_call_trait,
            ..
        } = Types::default();
        let self_ident = &self.ident;
        let call_fields = self.call_fields();
        let (imp, ty, wher) = self.generics.split_for_impl();
        let call_bounds = self.call_bounds();
        let wher = match wher {
            Some(w) => quote! { #w #(#call_bounds),* },
            None => quote! { where #(#call_bounds),* },
        };
        let wher = quote! {
            #wher, Self: #field_call_trait + #method_call_trait,
        };

        let builders = call_fields.into_iter().map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            let field_const_id = const_field_id(field_ident);
            let field_ty = &field.ty;
            let variant_name = to_camel_case(field_ident);
            quote! {
                impl #imp #build_call_trait <#field_const_id> for #self_ident #ty #wher {
                    type Child = #field_ty;
                    fn build_call<F: Fn(::orga::call::CallBuilder<Self::Child>) -> <Self::Child as #call_trait>::Call>(f: F, args: Self::Args) -> Self::Call {
                        let child_call = f(::orga::call::CallBuilder::new());
                        <Self as #call_trait>::Call::Field(<Self as #field_call_trait>::FieldCall::#variant_name(child_call) )
                    }
                }
            }
        });

        quote! {
            #(#builders)*
        }
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(forward_attrs)]
struct FieldCallFieldReceiver {
    ident: Option<Ident>,
    ty: Type,
    attrs: Vec<syn::Attribute>,
}
uses_type_params!(FieldCallFieldReceiver, ty);

impl FieldCallFieldReceiver {
    fn is_call(&self) -> bool {
        self.attrs
            .iter()
            .any(|attr| attr.path().segments.iter().any(|seg| seg.ident == "call"))
    }
}

impl ToTokens for FieldCallInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let fc_enum = self.field_call_enum();
        let fc_impl = self.field_call_impl(&fc_enum);
        let builders = self.call_builder();

        tokens.extend(quote! {
            #fc_enum

            #fc_impl

            #builders
        });
    }
}

struct FieldCallEnum {
    parent: FieldCallInputReceiver,
    ident: Ident,
    vis: Visibility,
    data: Vec<FieldCallFieldReceiver>,
}

impl ToTokens for FieldCallEnum {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Types {
            encode_trait,
            decode_trait,
            call_trait,
            state_trait,
            ed_result_ty,
            ed_error_ty,
            keyop_ty,
            ..
        } = Types::default();
        let ident = &self.ident;
        let parent_ident = &self.parent.ident;
        let vis = &self.vis;
        let variants = self.data.iter().map(|field| {
            let ident = to_camel_case(field.ident.as_ref().unwrap());
            let ty = &field.ty;
            let doctext = format!(
                "Field call for [{}::{}].",
                parent_ident,
                field.ident.as_ref().unwrap()
            );
            quote! {
                #[doc = #doctext]
                #ident(<#ty as #call_trait>::Call)
            }
        });

        let (imp, ty, wher) = self.parent.generics.split_for_impl();
        let _ident_str = ident.to_string();

        let unused_generics = self.parent.unused_generics();
        let unused_generics = unused_generics
            .declared_type_params()
            .into_iter()
            .map(|ty| quote! { #ty })
            .collect_vec();
        let unused_generics = match unused_generics.len() {
            0 => quote! { () },
            1 => quote! { #(#unused_generics),* },
            _ => quote! { (#(#unused_generics),*) },
        };

        let call_bounds = self.parent.call_bounds();

        let wher = match wher {
            Some(w) => quote! { #w #(#call_bounds),* },
            None => quote! { where #(#call_bounds),* },
        };

        let parent_ty = quote! { #parent_ident #ty };

        let enum_encode = {
            let child_encodes = self.data.iter().map(|field| {
                let cc_ident = to_camel_case(field.ident.as_ref().unwrap());
                let field_ident = field.ident.as_ref().unwrap();
                quote! {
                    #cc_ident(subcall) => {
                        if let Some(keyop) = <#parent_ty as #state_trait>::field_keyop(stringify!(#field_ident)) {
                            match keyop {
                                #keyop_ty::Absolute(_) => {
                                    // TODO: encode absolute keyops?
                                    Err(#ed_error_ty::UnencodableVariant)
                                },
                                #keyop_ty::Append(bytes) => {
                                    bytes.encode_into(out)?;
                                    subcall.encode_into(out)
                                }
                            }
                        } else {
                            Err(#ed_error_ty::UnencodableVariant)
                        }
                    }
                }
            });

            let child_encode_lens = self.data.iter().map(|field| {
                let cc_ident = to_camel_case(field.ident.as_ref().unwrap());
                let field_ident = field.ident.as_ref().unwrap();
                quote! {
                    #cc_ident(subcall) => {
                        if let Some(keyop) = <#parent_ty as #state_trait>::field_keyop(stringify!(#field_ident)) {
                            match keyop {
                                #keyop_ty::Absolute(_) => {
                                    // TODO: encode absolute keyops?
                                    Err(#ed_error_ty::UnencodableVariant)
                                },
                                #keyop_ty::Append(bytes) => {
                                    Ok(bytes.encoding_length()? + subcall.encoding_length()?)
                                }
                            }
                        } else {
                            Err(#ed_error_ty::UnencodableVariant)
                        }
                    }
                }
            });

            quote! {
                impl #imp #encode_trait for #ident #ty #wher {
                    fn encode_into<__W: ::std::io::Write>(&self, out: &mut __W) -> #ed_result_ty<()> {
                        match self {
                            #ident::Noop(_) => Ok(()),
                            #(#ident::#child_encodes,)*
                        }
                    }
                    fn encoding_length(&self) -> #ed_result_ty<usize> {
                        match self {
                            #ident::Noop(_) => Ok(0),
                            #(#ident::#child_encode_lens,)*
                        }
                    }
                }
            }
        };

        let enum_decode = {
            let child_decodes = self.data.iter().map(|field| {
                let cc_ident = to_camel_case(field.ident.as_ref().unwrap());
                let sc_ident = field.ident.as_ref().unwrap();
                quote! {
                    if let Some(#keyop_ty::Append(prefix)) = <#parent_ty as #state_trait>::field_keyop(stringify!(#sc_ident)) {
                       if bytes.starts_with(&prefix) {
                           let subcall = #decode_trait::decode(&mut &bytes[prefix.len()..])?;
                           return Ok(#ident::#cc_ident(subcall));
                       }
                    }
                }
            });

            quote! {
                impl #imp #decode_trait for #ident #ty #wher {
                    fn decode<__R: ::std::io::Read>(mut input: __R) -> #ed_result_ty<Self> {
                        let mut bytes = vec![];
                        input.read_to_end(&mut bytes)?;
                        #(#child_decodes)*

                        return Err(#ed_error_ty::UnexpectedByte(123))
                    }

                }
            }
        };

        let enum_debug = {
            let sc_parent_ident = to_snake_case(&self.parent.ident);
            let child_debugs = self.data.iter().map(|field| {
                let cc_ident = to_camel_case(field.ident.as_ref().unwrap());
                let sc_ident = field.ident.as_ref().unwrap().to_string();
                quote! {
                    #cc_ident(subcall) => {
                        if f.alternate() {
                            write!(f, "{}.", stringify!(#sc_parent_ident))?;
                        } else {
                            f.write_fmt(format_args!("."))?;
                        }
                        f.write_str(format!("{}{:?}", #sc_ident, &subcall).as_str())
                    }
                }
            });

            quote! {
                impl #imp ::std::fmt::Debug for #ident #ty #wher {
                    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {

                        match self {
                            #ident::Noop(_) => Ok(()),
                            #(#ident::#child_debugs,)*
                        }
                    }

                }
            }
        };
        let doctext = format!("Field calls for [{}].", parent_ident);

        tokens.extend(quote! {
            #[doc = #doctext]
            #vis enum #ident #imp #wher {
                /// No-op variant.
                Noop(::std::marker::PhantomData<fn(#unused_generics)>),
                #(#variants),*
            }

            #enum_encode

            #enum_decode

            #enum_debug

        });
    }
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    FieldCallInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
