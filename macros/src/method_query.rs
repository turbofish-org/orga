use crate::utils::{to_camel_case, to_snake_case};

use super::utils::{is_attr_with_ident, Types};
use darling::{usage::IdentSet, util::path_to_string, ToTokens};
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::*;

fn query_methods(item: &ItemImpl) -> Vec<&ImplItemFn> {
    item.items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Fn(method) = item {
                method
                    .attrs
                    .iter()
                    .any(|attr| is_attr_with_ident(attr, "query"))
                    .then(|| method)
            } else {
                None
            }
        })
        .collect()
}

fn self_ty_ident(item: &ItemImpl) -> Ident {
    if let Type::Path(TypePath { path, .. }) = &*item.self_ty.clone() {
        Ident::new(&path_to_string(&path), Span::call_site())
    } else {
        panic!("Expected a type path")
    }
}

fn method_args(method: &ImplItemFn) -> Vec<Type> {
    method
        .sig
        .inputs
        .iter()
        .skip(1)
        .map(|arg| {
            if let FnArg::Typed(PatType { ty, .. }) = arg {
                *ty.clone()
            } else {
                panic!("Expected a typed argument")
            }
        })
        .collect_vec()
}

fn enum_ident(item: &ItemImpl) -> Ident {
    format_ident!("{}{}", self_ty_ident(&item), "MethodQuery")
}

fn method_query_enum(tokens: &mut TokenStream2, item: &ItemImpl) {
    let Types {
        encode_trait,
        decode_trait,
        ..
    } = Types::default();
    let parent_ident = self_ty_ident(item);
    let sc_parent_ident = to_snake_case(&parent_ident);
    let ident = enum_ident(&item);
    let mut variants = query_methods(&item)
        .into_iter()
        .map(|method| {
            let ident = to_camel_case(&method.sig.ident);

            let mut args = method_args(method)
                .iter()
                .map(|ty| {
                    quote! { #ty }
                })
                .collect_vec();
            // let return_ty = if let ReturnType::Type(_, return_ty) = &method.sig.output {
            //     quote! { #return_ty }
            // } else {
            //     quote! { () }
            // };
            args.push(quote! { Vec<u8> });

            let doctext = format!("Method query for [{}::{}]", parent_ident, &method.sig.ident);
            quote! {
                #[doc = #doctext]
                #ident(#( #args ),*)
            }
        })
        .collect_vec();

    let (imp, ty, wher) = item.generics.split_for_impl();
    let tp = item
        .generics
        .type_params()
        .map(|param| {
            let ident = &param.ident;
            quote! { #ident }
        })
        .collect_vec();
    variants.push(quote! {
        /// No-op method query.
        Noop(::std::marker::PhantomData<fn((#( #tp ),*))>)
    });

    let _enum_debug = {
        let child_debugs = query_methods(&item).into_iter().map(|field| {
            let cc_ident = to_camel_case(&field.sig.ident);
            let sc_ident = &field.sig.ident;
            let args = method_args(field);
            let arg_names = args
                .iter()
                .enumerate()
                .map(|(i, _)| format_ident!("arg{}", i))
                .collect_vec();

            quote! {
                #cc_ident(#( #arg_names ),*) => {
                    if f.alternate() {
                        write!(f, "{}", stringify!(#sc_parent_ident))?;
                    }
                    write!(f, ".")?;

                    f.debug_tuple(stringify!(#sc_ident))
                        #( .field(&#arg_names) )*
                        .finish()
                }
            }
        });

        quote! {
            impl #imp ::std::fmt::Debug for #ident #ty #wher {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    use #ident::*;
                    match self {
                        #( #child_debugs ),*
                        Noop(_) => {Ok(())}
                    }
                }
            }
        }
    };

    let doctext = format!("Method query for [{}]", parent_ident);
    tokens.extend(quote! {
        #[doc = #doctext]
        #[derive(#encode_trait, #decode_trait, Debug)]
        pub enum #ident #ty {
            #( #variants ),*
        }

        // #enum_debug
    })
}

fn method_query_impl(tokens: &mut TokenStream2, item: &ItemImpl) {
    let Types {
        encode_trait,
        decode_trait,
        method_query_trait,
        query_trait,
        result_ty,
        ..
    } = Types::default();
    let ident = self_ty_ident(&item);
    let enum_ident = enum_ident(&item);
    let (imp, ty, wher) = item.generics.split_for_impl();
    let mut arms = query_methods(&item)
        .into_iter()
        .map(|method| {
            let ident = to_camel_case(&method.sig.ident);
            let args = method_args(method);
            let method_ident = &method.sig.ident;
            let arg_names = args
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let name = format_ident!("var{}", i);
                    quote! { #name }
                })
                .collect_vec();
            let mut param_names = arg_names.clone();
            param_names.push(quote! { subquery });
            quote! {
                #ident(#( #param_names ),*) => {
                    let result = self.#method_ident(#( #arg_names ),*);
                    if !subquery.is_empty() {
                        #query_trait::query(&result, #decode_trait::decode(subquery.as_slice())?)?;
                    }
                }
            }
        })
        .collect_vec();
    arms.push(quote! {
        Noop(_) => {}
    });

    let enum_bound = quote! { #enum_ident #ty: #encode_trait + #decode_trait + ::std::fmt::Debug };
    let wher = match wher {
        Some(w) => quote! { #w #enum_bound },
        None => quote! { where #enum_bound },
    };

    tokens.extend(quote! {
        impl #imp #method_query_trait for #ident #ty #wher {
            type MethodQuery = #enum_ident #ty;

            fn method_query(&self, query: Self::MethodQuery) -> #result_ty<()> {
                use #enum_ident::*;
                match query {
                    #( #arms ),*
                };

                Ok(())
            }
        }
    })
}

fn strip_query_attr(item: &mut ItemImpl) {
    for item in item.items.iter_mut() {
        if let ImplItem::Fn(method) = item {
            method
                .attrs
                .retain(|attr| !is_attr_with_ident(attr, "query"));
        }
    }
}

fn _method_arg_generics(item: &ItemImpl) -> Vec<Ident> {
    let mut arg_types = IdentSet::default();
    for method in query_methods(&item) {
        for arg in method_args(method) {
            if let Type::Path(TypePath { path, .. }) = arg {
                arg_types.extend(path.segments.iter().map(|seg| seg.ident.clone()));
            }
        }
    }

    item.generics
        .type_params()
        .map(|param| param.ident.clone())
        .filter(|ident| arg_types.contains(ident))
        .collect()
}

fn add_tracing(item_impl: &mut ItemImpl) {
    let Types {
        maybe_pop_trace_fn,
        maybe_push_trace_fn,
        trace_method_type_enum,
        encode_trait,
        ..
    } = Types::default();
    let mut query_index: u8 = 0x80;
    for item in item_impl.items.iter_mut() {
        if let ImplItem::Fn(method) = item {
            if method
                .attrs
                .iter()
                .any(|attr| is_attr_with_ident(attr, "query"))
            {
                let arg_names = method
                    .sig
                    .inputs
                    .iter()
                    .skip(1)
                    .map(|arg| match arg {
                        FnArg::Typed(PatType { pat, .. }) => {
                            if let Pat::Ident(PatIdent { ident, .. }) = &**pat {
                                ident.clone()
                            } else {
                                panic!("Expected an identifier")
                            }
                        }
                        _ => panic!("Expected a type path"),
                    })
                    .collect_vec();

                let encoded_args = if arg_names.is_empty() {
                    quote! { vec![] }
                } else {
                    quote! { vec![ #(#encode_trait::encode(&#arg_names).unwrap(),)*].concat() }
                };
                let mut stmts = vec![parse_quote! {
                    #maybe_push_trace_fn::<Self, _>( || (
                        #trace_method_type_enum::Query,
                        vec![#query_index],
                        #encoded_args,
                    ));
                }];
                query_index += 1;
                stmts.append(&mut method.block.stmts);
                method.block.stmts = stmts;
                let block = &method.block;

                method.block = parse_quote! {
                    { #maybe_pop_trace_fn(move || { #block }) }
                };

                let pfx = format!("{} (0x{:02x})", query_index - 1, query_index - 1);
                method.attrs.push(parse_quote! {
                   #[doc = "\n\n**Query prefix:**"]
                });
                method.attrs.push(parse_quote! {
                    #[doc = #pfx]
                });
            }
        }
    }
}

pub fn query_block(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item = syn::parse::<ItemImpl>(input.clone()).unwrap();
    add_tracing(&mut item);
    let query_methods = query_methods(&item);
    if query_methods.is_empty() {
        return input;
    }

    let mut tokens = quote! {}.into();
    method_query_enum(&mut tokens, &item);
    method_query_impl(&mut tokens, &item);
    strip_query_attr(&mut item);
    tokens.extend(item.into_token_stream());

    tokens.into()
}
