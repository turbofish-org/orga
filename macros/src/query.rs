use heck::{CamelCase, SnakeCase};
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use std::collections::HashSet;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);
    let name = &item.ident;
    let source = parse_parent();

    let mut query_type = None;
    let mut chainedquery_type = None;
    let enum_methodquery = create_enum_methodquery(&item, &source, &mut query_type);
    let enum_chainedquery = create_enum_chainedquery(&item, &source, &mut chainedquery_type);
    let query_impl = create_query_impl(
        &item,
        &source,
        query_type.unwrap(),
        chainedquery_type.unwrap(),
    );

    let output = quote!(
        use ::orga::macros::query;
        #enum_methodquery
        #enum_chainedquery
        #query_impl
    );

    println!("{}", &output);

    output.into()
}

pub fn attr(_: TokenStream, _: TokenStream) -> TokenStream {
    quote!().into()
}

fn create_query_impl(
    item: &DeriveInput,
    source: &File,
    query_ty: TokenStream2,
    chainedquery_ty: TokenStream2,
) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;
    let generic_params = gen_param_input(generics);

    quote! {
        impl#generics ::orga::query::MethodQuery for #name#generic_params {
            type Query = #query_ty;
            type ChainedQuery = #chainedquery_ty;

            fn method_query(&self, query: Self::Query) -> ::orga::Result<()> {
                todo!()
            }

            fn chained_query(&self, query: Self::ChainedQuery) -> ::orga::Result<()> {
                todo!()
            }
        }
    }
}

fn create_enum_methodquery(
    item: &DeriveInput,
    source: &File,
    ty: &mut Option<TokenStream2>,
) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;

    let mut generic_params = vec![];

    let variants: Vec<_> = relevant_methods(name, "query", source)
        .into_iter()
        .map(|method| {
            let name = method.sig.ident.to_string();
            let name_camel = name.as_str().to_camel_case();
            let name = Ident::new(&name_camel, Span::call_site());

            let fields = if method.sig.inputs.len() == 1 {
                quote!()
            } else {
                let inputs: Vec<_> = method
                    .sig
                    .inputs
                    .iter()
                    .skip(1)
                    .map(|input| match input {
                        FnArg::Typed(input) => *input.ty.clone(),
                        _ => panic!("unexpected input"),
                    })
                    .collect();

                let requirements = get_generic_requirements(
                    inputs.iter().cloned(),
                    generics.params.iter().cloned(),
                );
                generic_params.extend(requirements);

                quote! { (#(#inputs),*) }
            };

            quote! {
                #name#fields
            }
        })
        .collect();

    let generic_params = if generic_params.is_empty() {
        quote!()
    } else {
        let params: HashSet<_> = generic_params.into_iter().collect();
        let params = params.into_iter();
        quote!(<#(#params),*>)
    };

    *ty = Some(quote!(MethodQuery#generic_params));

    quote! {
        #[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
        pub enum #ty {
            #(#variants,)*
        }
    }
}

fn create_enum_chainedquery(
    item: &DeriveInput,
    source: &File,
    ty: &mut Option<TokenStream2>,
) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;

    let mut generic_params = vec![];

    let variants: Vec<_> = relevant_methods(name, "query", source)
        .into_iter()
        .filter(|method| matches!(method.sig.output, ReturnType::Type(_, _)))
        .map(|method| {
            let name = method.sig.ident.to_string();
            let name_camel = name.as_str().to_camel_case();
            let name = Ident::new(&name_camel, Span::call_site());

            let fields = if method.sig.inputs.len() == 1 {
                quote!()
            } else {
                let inputs: Vec<_> = method
                    .sig
                    .inputs
                    .iter()
                    .skip(1)
                    .map(|input| match input {
                        FnArg::Typed(input) => *input.ty.clone(),
                        _ => panic!("unexpected input"),
                    })
                    .collect();

                let requirements = get_generic_requirements(
                    inputs.iter().cloned(),
                    generics.params.iter().cloned(),
                );
                generic_params.extend(requirements);

                quote! { #(#inputs),*, }
            };

            let output_type = match method.sig.output {
                ReturnType::Type(_, ref ty) => *(ty.clone()),
                ReturnType::Default => panic!("unexpected return type"),
            };
            let subquery = quote!(<#output_type as ::orga::query::Query>::Query);

            let requirements = get_generic_requirements(
                vec![output_type].iter().cloned(),
                generics.params.iter().cloned(),
            );
            generic_params.extend(requirements);

            quote! {
                #name(#fields #subquery)
            }
        })
        .collect();

    let generic_params = if generic_params.is_empty() {
        quote!()
    } else {
        let params: HashSet<_> = generic_params.into_iter().collect();
        let params = params.into_iter();
        quote!(<#(#params),*>)
    };

    *ty = Some(quote!(ChainedQuery#generic_params));

    quote! {
        #[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
        pub enum #ty {
            #(#variants,)*
        }
    }
}

fn parse_parent() -> File {
    let path = proc_macro::Span::call_site().source_file().path();
    let source = std::fs::read_to_string(path).unwrap();
    parse_file(source.as_str()).unwrap()
}

fn get_generic_requirements<I, J>(inputs: I, params: J) -> Vec<Ident>
where
    I: Iterator<Item = Type>,
    J: Iterator<Item = GenericParam>,
{
    let params = params.collect::<Vec<_>>();
    let maybe_generic_inputs = inputs.filter_map(|input| match input {
        Type::Path(path) => Some(path),
        _ => None,
    });
    let mut requirements = vec![];
    for input in maybe_generic_inputs {
        params
            .iter()
            .filter_map(|param| match param {
                GenericParam::Type(param) => Some(param),
                _ => None,
            })
            .find(|param| {
                param.ident == input.path.segments.last().unwrap().ident
            })
            .map(|param| {
                requirements.push(param.ident.clone());
            });
    }
    requirements
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
                method
                    .attrs
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

fn gen_param_input(generics: &Generics) -> TokenStream2 {
    let gen_params = generics.params.iter().map(|p| match p {
        GenericParam::Type(p) => {
            let ident = &p.ident;
            quote!(#ident)
        }
        GenericParam::Lifetime(p) => {
            let ident = &p.lifetime.ident;
            quote!(#ident)
        }
        GenericParam::Const(p) => {
            let ident = &p.ident;
            quote!(#ident)
        }
    });

    if gen_params.len() == 0 {
        quote!()
    } else {
        quote!(<#(#gen_params),*>)
    }
}
