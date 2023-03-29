use crate::utils::{to_camel_case, to_snake_case};

use super::utils::{is_attr_with_ident, Types};
use darling::{usage::IdentSet, util::path_to_string, ToTokens};
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::*;

fn call_methods(item: &ItemImpl) -> Vec<&ImplItemMethod> {
    item.items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Method(method) = item {
                method
                    .attrs
                    .iter()
                    .any(|attr| is_attr_with_ident(attr, "call"))
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

fn method_args(method: &ImplItemMethod) -> Vec<Type> {
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
    format_ident!("{}{}", self_ty_ident(&item), "MethodCall")
}

fn method_call_enum(tokens: &mut TokenStream2, item: &ItemImpl) {
    let Types {
        encode_trait,
        decode_trait,
        ..
    } = Types::default();
    let parent_ident = self_ty_ident(item);
    let sc_parent_ident = to_snake_case(&parent_ident);
    let ident = enum_ident(&item);
    let variants = call_methods(&item).into_iter().map(|method| {
        let ident = to_camel_case(&method.sig.ident);

        let args = method_args(method);

        quote! { #ident(#( #args ),*) }
    });
    let generics = method_arg_generics(&item);
    let generics = quote! { <#(#generics),*> };

    let enum_debug = {
        let child_debugs = call_methods(&item).into_iter().map(|field| {
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
                    if !f.alternate() {
                        write!(f, "{}.", stringify!(#sc_parent_ident))?;
                    }

                    f.debug_tuple(stringify!(#sc_ident))
                        #( .field(&#arg_names) )*
                        .finish()
                }
            }
        });

        quote! {
            impl #generics ::std::fmt::Debug for #ident #generics {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    use #ident::*;
                    match self {
                        #( #child_debugs ),*
                    }
                }
            }
        }
    };

    tokens.extend(quote! {
        #[derive(#encode_trait, #decode_trait)]
        pub enum #ident #generics {
            #( #variants ),*
        }

        #enum_debug
    })
}

fn method_call_impl(tokens: &mut TokenStream2, item: &ItemImpl) {
    let Types {
        encode_trait,
        decode_trait,
        method_call_trait,
        method_call_marker_trait,
        result_ty,
        ..
    } = Types::default();
    let ident = self_ty_ident(&item);
    let enum_ident = enum_ident(&item);
    let (imp, ty, wher) = item.generics.split_for_impl();
    let arms = call_methods(&item).into_iter().map(|method| {
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

        quote! {
            #ident(#( #arg_names ),*) => {
                self.#method_ident(#( #arg_names ),*)?;
            }
        }
    });
    let enum_generics = method_arg_generics(&item);
    let enum_generics = quote! { <#(#enum_generics),*> };
    let enum_bound =
        quote! { #enum_ident #enum_generics: #encode_trait + #decode_trait + ::std::fmt::Debug };
    let wher = match wher {
        Some(w) => quote! { #w #enum_bound },
        None => quote! { where #enum_bound },
    };

    tokens.extend(quote! {
        impl #imp #method_call_trait for #ident #ty #wher {
            type MethodCall = #enum_ident #enum_generics;

            fn method_call(&mut self, call: Self::MethodCall) -> #result_ty<()> {
                use #enum_ident::*;
                match call {
                    #( #arms ),*
                };

                Ok(())
            }
        }

        impl #ty !#method_call_marker_trait for #ident #ty {}
    })
}

fn strip_call_attr(item: &mut ItemImpl) {
    for item in item.items.iter_mut() {
        if let ImplItem::Method(method) = item {
            method
                .attrs
                .retain(|attr| !is_attr_with_ident(attr, "call"));
        }
    }
}

fn method_arg_generics(item: &ItemImpl) -> Vec<Ident> {
    let mut arg_types = IdentSet::default();
    for method in call_methods(&item) {
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
        trace_fn,
        encode_trait,
        ..
    } = Types::default();
    let mut call_index: u8 = 0x40;
    for item in item_impl.items.iter_mut() {
        if let ImplItem::Method(method) = item {
            if method
                .attrs
                .iter()
                .any(|attr| is_attr_with_ident(attr, "call"))
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
                    quote! { vec![ #(#encode_trait::encode(&#arg_names)?,)*].concat() }
                };
                let mut stmts = vec![parse_quote! {
                    #trace_fn::<Self>(vec![#call_index], #encoded_args)?;
                }];
                call_index += 1;
                stmts.append(&mut method.block.stmts);
                method.block.stmts = stmts;
                let block = &method.block;

                method.block = parse_quote! {
                    #block
                };

                let pfx = format!("{} (0x{:02x})", call_index - 1, call_index - 1);
                method.attrs.push(parse_quote! {
                   #[doc = "\n\n**Call prefix:**"]
                });
                method.attrs.push(parse_quote! {
                    #[doc = #pfx]
                });
            }
        }
    }
}

pub fn call_block(args: TokenStream, input: TokenStream) -> TokenStream {
    let _attr_args = parse_macro_input!(args as AttributeArgs);
    let mut item = syn::parse::<ItemImpl>(input.clone()).unwrap();
    add_tracing(&mut item);
    let call_methods = call_methods(&item);
    if call_methods.is_empty() {
        return input;
    }

    let mut tokens = quote! {}.into();
    method_call_enum(&mut tokens, &item);
    method_call_impl(&mut tokens, &item);
    strip_call_attr(&mut item);
    tokens.extend(item.into_token_stream());

    tokens.into()
}
