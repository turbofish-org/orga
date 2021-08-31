use proc_macro::TokenStream;
use proc_macro2::{Literal, TokenStream as TokenStream2, Span};
use quote::{quote, ToTokens};
use syn::*;
use heck::{CamelCase, SnakeCase};

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);
    let name = &item.ident;
    let source = parse_parent();

    let enum_methodquery = create_enum_methodquery(name, &source);
    let enum_chainedquery = create_enum_chainedquery(name, &source);

    let output = quote!(
        use ::orga::macros::query;
        #enum_methodquery
        #enum_chainedquery
    );

    println!("{}", &output);

    output.into()
}

pub fn attr(_: TokenStream, _: TokenStream) -> TokenStream {
    quote!().into()
}

fn create_enum_methodquery(name: &Ident, source: &File) -> TokenStream2 {
    let variants = relevant_methods(name, "query", source)
        .into_iter()
        .map(|method| {
            let name = method.sig.ident.to_string();
            let name_camel = name.as_str().to_camel_case();
            let name = Ident::new(&name_camel, Span::call_site());

            let fields = if method.sig.inputs.len() == 1 {
                quote!()
            } else {
                let inputs = method.sig.inputs
                    .iter()
                    .skip(1)
                    .map(|input| match input {
                        FnArg::Typed(input) => input.ty.clone(),
                        _ => panic!("unexpected input"),
                    });
                quote! { (#(#inputs),*) }
            };

            quote! {
                #name#fields
            }
        });

    quote! {
        #[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
        pub enum MethodQuery {
            #(#variants,)*
        }
    }
}

fn create_enum_chainedquery(name: &Ident, source: &File) -> TokenStream2 {
    let variants = relevant_methods(name, "query", source)
        .into_iter()
        .filter(|method| matches!(method.sig.output, ReturnType::Type(_, _)))
        .map(|method| {
            let name = method.sig.ident.to_string();
            let name_camel = name.as_str().to_camel_case();
            let name = Ident::new(&name_camel, Span::call_site());        

            let fields = if method.sig.inputs.len() == 1 {
                quote!()
            } else {
                let inputs = method.sig.inputs
                    .iter()
                    .skip(1)
                    .map(|input| match input {
                        FnArg::Typed(input) => input.ty.clone(),
                        _ => panic!("unexpected input"),
                    });
                quote! { #(#inputs),*, }
            };

            let output_type = match method.sig.output {
                ReturnType::Type(_, ref ty) => ty.clone(),
                ReturnType::Default => panic!("unexpected return type"),
            };
            let subquery = quote!(<#output_type as ::orga::query::Query>::Query);

            quote! {
                #name(#fields #subquery)
            }
        });

    quote! {
        #[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
        pub enum ChainedQuery {
            #(#variants,)*
        }
    }
}

fn parse_parent() -> File {
    let path = proc_macro::Span::call_site().source_file().path();
    let source = std::fs::read_to_string(path).unwrap();
    parse_file(source.as_str()).unwrap()
}

fn relevant_impls<'a>(name: &Ident, source: &'a File) -> Vec<&'a ItemImpl> {
    source
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(item) => Some(item),
            _ => None,
        })
        .filter(|item| item.trait_.is_none())
        .filter(|item| {
            let path = match &*item.self_ty {
                Type::Path(path) => path,
                _ => return false,
            };

            if path.qself.is_some() {
                return false;
            }
            if path.path.segments.len() != 1 {
                return false;
            }
            if path.path.segments[0].ident != *name {
                return false;
            }

            true
        })
        .collect()
}

fn relevant_methods<'a>(name: &Ident, attr: &str, source: &'a File) -> Vec<&'a ImplItemMethod> {
    let get_methods = |item: &'a ItemImpl| {
        item.items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Method(method) => Some(method),
                _ => None,
            })
            .filter(|method| {
                method.attrs
                    .iter()
                    .find(|a| a.path.is_ident(&attr))
                    .is_some()
            })
            .filter(|method| matches!(method.vis, Visibility::Public(_)))
            .filter(|method| method.sig.unsafety.is_none())
            .filter(|method| method.sig.asyncness.is_none())
            .filter(|method| method.sig.abi.is_none())
    };

    relevant_impls(name, source)
        .into_iter()
        .flat_map(get_methods)
        .collect()
}
