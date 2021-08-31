use heck::{CamelCase, SnakeCase};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use std::collections::HashSet;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);
    let source = parse_parent();

    let name = &item.ident;
    let modname = Ident::new(
        format!("{}_query", name).to_snake_case().as_str(),
        Span::call_site(),
    );

    let query_enum = create_query_enum(&item, &source);
    let query_impl = create_query_impl(&item, &source, &query_enum);

    let output = quote!(
        use ::orga::macros::query;

        pub mod #modname {
            use super::*;
            #query_enum
            #query_impl
        }
    );

    println!("{}", &output.to_string());

    output.into()
}

pub fn attr(args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

fn create_query_impl(
    item: &DeriveInput,
    source: &File,
    query_enum: &ItemEnum,
) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;
    let generic_params = gen_param_input(generics);

    let query_type = &query_enum.ident;
    let query_generics = &query_enum.generics;
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);
    
    let encoding_bounds = relevant_methods(name, "query", source)
        .into_iter()
        .flat_map(|method| {
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

            get_generic_requirements(
                inputs.iter().cloned(),
                item.generics.params.iter().cloned(),
            )
        })
        .map(|p| quote!(#p: ::orga::encoding::Encode + ::orga::encoding::Decode + ::orga::encoding::Terminated,));
    let encoding_bounds = quote!(#(#encoding_bounds)*);

    let query_bounds = relevant_methods(name, "query", source)
        .into_iter()
        .map(|method| {
            let unit_tuple: Type = parse2(quote!(())).unwrap();
            match method.sig.output {
                ReturnType::Type(_, ref ty) => *(ty.clone()),
                ReturnType::Default => unit_tuple,
            }
        })
        .map(|t| quote!(#t: ::orga::query::Query,));
    let query_bounds = quote!(#(#query_bounds)*);

    let field_query_arms = vec![quote!()];

    let method_query_arms: Vec<_> = relevant_methods(name, "query", source)
        .into_iter()
        .map(|method| {
            let method_name = &method.sig.ident;
            let name_camel = method_name.to_string().to_camel_case();
            let variant_name = Ident::new(&name_camel, Span::call_site());

            let inputs: Vec<_> = (0..method.sig.inputs.len() - 1)
                .into_iter()
                .map(|i| Ident::new(
                    format!("var{}", i).as_str(),
                    Span::call_site(),
                ))
                .collect();

            let unit_tuple: Type = parse2(quote!(())).unwrap();
            let output_type = match method.sig.output {
                ReturnType::Type(_, ref ty) => *(ty.clone()),
                ReturnType::Default => unit_tuple,
            };
            let subquery = quote!(<#output_type as ::orga::query::Query>::Query);
            
            quote! {
                Query::#variant_name(#(#inputs,)* subquery) => {
                    self.#method_name(#(#inputs),*).query(subquery)
                }
            }
        })
        .collect();

    quote! {
        impl#generics ::orga::query::Query for #name#generic_params
        where #where_preds #encoding_bounds #query_bounds
        {
            type Query = #query_type#query_generics;

            fn query(&self, query: Self::Query) -> ::orga::Result<()> {
                match query {
                    Query::This => Ok(()),
                    #(#field_query_arms),*
                    #(#method_query_arms),*
                }
            }
        }
    }
}

fn create_query_enum(item: &DeriveInput, source: &File) -> ItemEnum {
    let name = &item.ident;
    let generics = &item.generics;

    let mut generic_params = vec![];

    let method_variants: Vec<_> = relevant_methods(name, "query", source)
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

                quote! { #(#inputs),*, }
            };

            let unit_tuple: Type = parse2(quote!(())).unwrap();
            let output_type = match method.sig.output {
                ReturnType::Type(_, ref ty) => *(ty.clone()),
                ReturnType::Default => unit_tuple,
            };
            let subquery = quote!(<#output_type as ::orga::query::Query>::Query);
            
            let requirements = get_generic_requirements(
                vec![ output_type ].iter().cloned(),
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

    let where_clause = if item.generics.params.is_empty() {
        quote!()
    } else {
        let params = item.generics.params
            .iter()
            .filter_map(|p| match p {
                GenericParam::Type(t) => Some(t),
                _ => None,
            })
            .map(|p| &p.ident);
        quote! {
            where
                #(#params: ::orga::query::Query,)*
        }
    };

    let output = quote! {
        #[derive(::orga::encoding::Encode, ::orga::encoding::Decode)]
        pub enum Query#generic_params
        #where_clause
        {
            This,
            #(#method_variants,)*
        }
    };

    syn::parse2(output).unwrap()
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
