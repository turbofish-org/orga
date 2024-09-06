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

use crate::utils::{to_camel_case, to_snake_case, Types};

#[derive(Debug, Clone, FromDeriveInput)]
#[darling(supports(struct_named), forward_attrs)]
struct FieldQueryInputReceiver {
    ident: Ident,
    generics: Generics,
    vis: Visibility,
    data: ast::Data<(), FieldQueryFieldReceiver>,
}

impl FieldQueryInputReceiver {
    fn query_fields(&self) -> Vec<FieldQueryFieldReceiver> {
        self.data
            .as_ref()
            .take_struct()
            .unwrap()
            .fields
            .clone()
            .into_iter()
            .filter(|field| field.is_query())
            .cloned()
            .collect()
    }

    fn query_generics(&self) -> Generics {
        let Types { query_trait, .. } = Types::default();
        let mut ret_generics = self.generics.clone();
        let qf = self.query_fields();
        let params = qf.collect_type_params_cloned(
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
        for field in qf.iter() {
            let ty = &field.ty;
            wher.predicates.push(parse_quote!(#ty: #query_trait))
        }
        ret_generics
    }

    fn unused_generics(&self) -> Generics {
        let query_generics = self.query_generics();
        let query_generics = query_generics.type_params().collect::<HashSet<_>>();

        let mut ret_generics = self.generics.clone();
        ret_generics.params = ret_generics
            .params
            .into_iter()
            .filter(|gp| match gp {
                GenericParam::Type(ref ty) => !query_generics.contains(ty),
                _ => true,
            })
            .collect();

        ret_generics
    }

    fn field_query_enum(&self) -> FieldQueryEnum {
        FieldQueryEnum {
            parent: self.clone(),
            vis: self.vis.clone(),
            ident: Ident::new(&format!("{}FieldQuery", self.ident), Span::call_site()),
            data: self.query_fields(),
        }
    }

    fn query_bounds(&self) -> Vec<TokenStream2> {
        let Types {
            state_trait,
            query_trait,
            ..
        } = Types::default();
        let mut bounds = self
            .query_fields()
            .into_iter()
            .map(|field| {
                let ty = &field.ty;
                quote! { #ty: #query_trait }
            })
            .collect_vec();

        let (_, ty, _) = self.generics.split_for_impl();
        let ident = &self.ident;

        bounds.push(quote! { #ident #ty: #state_trait});

        bounds
    }

    fn field_query_impl(&self, fq_enum: &FieldQueryEnum) -> TokenStream2 {
        let Types {
            result_ty,
            field_query_trait,
            query_trait,
            ..
        } = Types::default();
        let ident = &self.ident;
        let fq_enum_ident = &fq_enum.ident;
        let (imp, ty, wher) = self.generics.split_for_impl();
        let arms = fq_enum.data.iter().map(|v| {
            let cc_ident = to_camel_case(v.ident.as_ref().unwrap());
            let sc_ident = v.ident.as_ref().unwrap();
            quote! { #cc_ident(subquery) => #query_trait::query(&self.#sc_ident, subquery) }
        });

        let query_bounds = self.query_bounds();
        let wher = match wher {
            Some(w) => quote! { #w #(#query_bounds),* },
            None => quote! { where #(#query_bounds),* },
        };

        quote! {
            impl #imp #field_query_trait for #ident #ty #wher {
                type FieldQuery = #fq_enum_ident #ty;
                fn field_query(&self, query: Self::FieldQuery) -> #result_ty<()> {
                    use #fq_enum_ident::*;
                    match query {
                        #(#arms,)*
                        Noop(_) => Ok(()),
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(forward_attrs)]
struct FieldQueryFieldReceiver {
    ident: Option<Ident>,
    ty: Type,
    vis: Visibility,
}
uses_type_params!(FieldQueryFieldReceiver, ty);

impl FieldQueryFieldReceiver {
    fn is_query(&self) -> bool {
        matches!(self.vis, Visibility::Public(_))
    }
}

impl ToTokens for FieldQueryInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let fq_enum = self.field_query_enum();
        let fq_impl = self.field_query_impl(&fq_enum);

        tokens.extend(quote! {
            #fq_enum

            #fq_impl
        });
    }
}

struct FieldQueryEnum {
    parent: FieldQueryInputReceiver,
    ident: Ident,
    vis: Visibility,
    data: Vec<FieldQueryFieldReceiver>,
}

impl ToTokens for FieldQueryEnum {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Types {
            encode_trait,
            decode_trait,
            query_trait,
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
                "Field query for [{}::{}].",
                parent_ident,
                field.ident.as_ref().unwrap()
            );
            quote! {
                #[doc = #doctext]
                #ident(<#ty as #query_trait>::Query)
            }
        });

        let (imp, ty, wher) = self.parent.generics.split_for_impl();

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

        let query_bounds = self.parent.query_bounds();

        let wher = match wher {
            Some(w) => quote! { #w #(#query_bounds),* },
            None => quote! { where #(#query_bounds),* },
        };

        let parent_ty = quote! { #parent_ident #ty };

        let enum_encode = {
            let child_encodes = self.data.iter().map(|field| {
                let cc_ident = to_camel_case(field.ident.as_ref().unwrap());
                let field_ident = field.ident.as_ref().unwrap();
                quote! {
                    #cc_ident(subquery) => {
                        if let Some(keyop) = <#parent_ty as #state_trait>::field_keyop(stringify!(#field_ident)) {
                            match keyop {
                                #keyop_ty::Absolute(_) => {
                                    // TODO: encode absolute keyops?
                                    Err(#ed_error_ty::UnencodableVariant)
                                },
                                #keyop_ty::Append(bytes) => {
                                    bytes.encode_into(out)?;
                                    subquery.encode_into(out)
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
                    #cc_ident(subquery) => {
                        if let Some(keyop) = <#parent_ty as #state_trait>::field_keyop(stringify!(#field_ident)) {
                            match keyop {
                                #keyop_ty::Absolute(_) => {
                                    // TODO: encode absolute keyops?
                                    Err(#ed_error_ty::UnencodableVariant)
                                },
                                #keyop_ty::Append(bytes) => {
                                    Ok(bytes.encoding_length()? + subquery.encoding_length()?)
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
                           let subquery = #decode_trait::decode(&mut &bytes[prefix.len()..])?;
                           return Ok(#ident::#cc_ident(subquery));
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

        let _enum_debug = {
            let sc_parent_ident = to_snake_case(&self.parent.ident);
            let child_debugs = self.data.iter().map(|field| {
                let cc_ident = to_camel_case(field.ident.as_ref().unwrap());
                let sc_ident = field.ident.as_ref().unwrap().to_string();
                quote! {
                    #cc_ident(subquery) => {
                        if f.alternate() {
                            write!(f, "{}.", stringify!(#sc_parent_ident))?;
                        } else {
                            f.write_fmt(format_args!("."))?;
                        }
                        f.write_str(format!("{}{:?}", #sc_ident, &subquery).as_str())
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

        let doctext = format!("Field queries for [{}].", parent_ident);
        tokens.extend(quote! {
            #[derive(::orga::Educe)]
            #[educe(Debug)]
            #[doc = #doctext]
            #vis enum #ident #imp #wher {
                /// No-op variant.
                Noop(::std::marker::PhantomData<fn(#unused_generics)>),
                #(#variants),*
            }

            #enum_encode

            #enum_decode

            // #enum_debug

        });
    }
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    FieldQueryInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
