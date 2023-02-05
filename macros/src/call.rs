use heck::{CamelCase, SnakeCase};
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::quote;
use std::collections::HashSet;
use syn::*;

use super::utils::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);
    let source = parse_parent();

    let name = &item.ident;
    let modname = Ident::new(
        format!("{}_call", name).to_snake_case().as_str(),
        Span::call_site(),
    );

    let (call_enum_tokens, call_enum) = create_call_enum(&item, &source);
    let (call_impl_tokens, _) = create_call_impl(&item, &source, &call_enum);

    let output = quote!(
        use ::orga::macros::*;
        pub mod #modname {
            use super::*;

            #call_enum_tokens
            #call_impl_tokens
        }
    );

    output.into()
}

pub fn attr(_args: TokenStream, input: TokenStream) -> TokenStream {
    let method = parse_macro_input!(input as ImplItemMethod);

    if !matches!(method.vis, Visibility::Public(_)) {
        panic!("Call methods must be public");
    }

    if method.sig.unsafety.is_some() {
        panic!("Call methods cannot be unsafe");
    }

    if method.sig.asyncness.is_some() {
        panic!("Call methods cannot be async");
    }

    if method.sig.abi.is_some() {
        panic!("Call methods cannot specify ABI");
    }

    quote!(#method).into()
}

pub(super) fn create_call_impl(
    item: &DeriveInput,
    source: &File,
    call_enum: &ItemEnum,
) -> (TokenStream2, ItemImpl) {
    let name = &item.ident;
    let generics = &item.generics;
    let mut generics_sanitized = generics.clone();
    generics_sanitized.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
        }
    });
    let generic_params = gen_param_input(generics, true);

    let call_type = &call_enum.ident;
    let call_generics = &call_enum.generics;
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);

    let encoding_bounds = relevant_methods(name, "call", source)
        .into_iter()
        .flat_map(|(method, _)| {
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

    let call_bounds = relevant_methods(name, "call", source)
        .into_iter()
        .map(|(method, _)| {
            let unit_tuple: Type = parse2(quote!(())).unwrap();
            match method.sig.output {
                ReturnType::Type(_, ref ty) => *(ty.clone()),
                ReturnType::Default => unit_tuple,
            }
        })
        .flat_map(|ty| {
            get_generic_requirements(
                std::iter::once(&ty).cloned(),
                generics.params.iter().cloned(),
            )
        })
        .map(|t| quote!(#t: ::orga::call::Call,));
    let call_bounds = quote!(#(#call_bounds)*);

    let parameter_bounds = relevant_methods(name, "call", source)
        .into_iter()
        .flat_map(|(method, _)| {
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

            inputs
        })
        .map(|t| quote!(#t: std::fmt::Debug,))
        .collect::<Vec<_>>();
    let parameter_bounds = quote!(#(#parameter_bounds)*);

    let fields = match &item.data {
        Data::Struct(data) => data.fields.iter(),
        Data::Enum(_) => todo!("Enums are not supported yet"),
        Data::Union(_) => panic!("Unions are not supported"),
    };
    let field_call_arms: Vec<_> = fields
        .filter(|field| {
            let field_names: Vec<String> = field
                .attrs
                .iter()
                .map(|attr| attr.path.get_ident().unwrap().to_string())
                .collect();

            field_names.contains(&("call".into()))
        })
        .enumerate()
        .map(|(i, field)| {
            let variant_name = field.ident.as_ref().map_or(
                Ident::new(format!("Field{}", i).as_str(), Span::call_site()),
                |f| {
                    Ident::new(
                        format!("Field{}", f.to_string().to_camel_case()).as_str(),
                        Span::call_site(),
                    )
                },
            );
            let field_name = field.ident.as_ref().map_or_else(
                || {
                    let i = Literal::usize_unsuffixed(i);
                    quote!(#i)
                },
                |f| quote!(#f),
            );

            quote! {
                Call::#variant_name(subcall) => {
                    ::orga::call::maybe_call(&mut self.#field_name, subcall)
                }
            }
        })
        .collect();

    let mut maybe_call_defs = vec![];
    let method_call_arms: Vec<_> = relevant_methods(name, "call", source)
        .into_iter()
        .map(|(method, parent)| {
            let method_name = &method.sig.ident;

            let name_camel = method_name.to_string().to_camel_case();
            let variant_name =
                Ident::new(format!("Method{}", &name_camel).as_str(), Span::call_site());

            let inputs: Vec<_> = (1..method.sig.inputs.len())
                .into_iter()
                .map(|i| Ident::new(format!("var{}", i).as_str(), Span::call_site()))
                .collect();
            let input_types: Vec<_> = method
                .sig
                .inputs
                .iter()
                .skip(1)
                .filter_map(|input| match input {
                    FnArg::Typed(input) => Some(*input.ty.clone()),
                    _ => None,
                })
                .collect();
            let full_inputs = quote! {
                #(, #inputs: #input_types)*, subcall: Vec<u8>
            };

            let unit_tuple: Type = parse2(quote!(())).unwrap();
            let output_type = match method.sig.output {
                ReturnType::Type(_, ref ty) => *(ty.clone()),
                ReturnType::Default => unit_tuple,
            };

            let requirements = get_generic_requirements(
                input_types
                    .iter()
                    .chain(std::iter::once(&output_type))
                    .cloned(),
                generics.params.iter().cloned(),
            );
            let generic_reqs = if requirements.is_empty() {
                quote!()
            } else {
                quote!(<#(#requirements),*>)
            };

            let parent_generics = &parent.generics;
            let parent_where_preds = &parent.generics.where_clause.as_ref().map(|w| &w.predicates);

            let trait_name = Ident::new(
                format!("MaybeCall{}", &variant_name).as_str(),
                Span::call_site(),
            );
            maybe_call_defs.push(quote! {
                trait #trait_name#generic_reqs {
                    fn maybe_call(&mut self #full_inputs) -> ::orga::Result<()>;
                }
                impl<__Self, #(#requirements),*> #trait_name#generic_reqs for __Self {
                    default fn maybe_call(&mut self #full_inputs) -> ::orga::Result<()> {
                        Err(::orga::Error::Call("This call cannot be called because not all bounds are met".into()))
                    }
                }
                impl#parent_generics #trait_name#generic_reqs for #name#generic_params
                where #where_preds #encoding_bounds #call_bounds #parent_where_preds
                {
                    fn maybe_call(&mut self #full_inputs) -> ::orga::Result<()> {
                        let output = self.#method_name(#(#inputs),*);
                        ::orga::call::maybe_call(output, subcall)
                    }
                }
            });

            let dotted_generic_reqs = if generic_reqs.is_empty() {
                quote!()
            } else {
                quote!(::#generic_reqs)
            };

            quote! {
                Call::#variant_name(#(#inputs,)* subcall) => {
                    #trait_name#dotted_generic_reqs::maybe_call(self, #(#inputs,)* subcall)
                }
            }
        })
        .collect();

    let impl_output = quote! {
        impl#generics_sanitized ::orga::call::Call for #name#generic_params
        where #where_preds #encoding_bounds #parameter_bounds
        {
            type Call = #call_type#call_generics;

            fn call(&mut self, call: Self::Call) -> ::orga::Result<()> {
                match call {
                    #call_type::Noop => Ok(()),
                    #(#field_call_arms),*
                    #(#method_call_arms),*
                }
            }
        }
    };

    let output = quote! {
        #impl_output
        #(#maybe_call_defs)*
    };

    (output, syn::parse2(impl_output).unwrap())
}

pub(super) fn create_call_enum(item: &DeriveInput, source: &File) -> (TokenStream2, ItemEnum) {
    let name = &item.ident;
    let generics = &item.generics;

    let mut generic_params = vec![];

    let fields = match &item.data {
        Data::Struct(data) => data.fields.iter(),
        Data::Enum(_) => todo!("Enums are not supported yet"),
        Data::Union(_) => panic!("Unions are not supported"),
    };
    let field_variants: Vec<_> = fields
        .filter(|field| {
            let field_names: Vec<String> = field
                .attrs
                .iter()
                .map(|attr| attr.path.get_ident().unwrap().to_string())
                .collect();

            field_names.contains(&("call".into()))
        })
        .enumerate()
        .map(|(i, field)| {
            let name = field.ident.as_ref().map_or(
                Ident::new(format!("Field{}", i).as_str(), Span::call_site()),
                |f| {
                    Ident::new(
                        format!("Field{}", f.to_string().to_camel_case()).as_str(),
                        Span::call_site(),
                    )
                },
            );

            let requirements = get_generic_requirements(
                vec![field.ty.clone()].into_iter(),
                generics.params.iter().cloned(),
            );
            generic_params.extend(requirements.clone());

            quote!(#name(Vec<u8>))
        })
        .collect();

    let method_variants: Vec<_> = relevant_methods(name, "call", source)
        .into_iter()
        .map(|(method, _)| {
            let name_camel = method.sig.ident.to_string().to_camel_case();
            let name = Ident::new(format!("Method{}", &name_camel).as_str(), Span::call_site());

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

            quote! {
                #name(#fields Vec<u8>)
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

    let struct_output = quote! {
        #[derive(::orga::encoding::Encode, ::orga::encoding::Decode, std::fmt::Debug)]
        pub enum Call#generic_params {
            Noop,
            #(#field_variants,)*
            #(#method_variants,)*
        }
    };

    let output = quote! {
        #struct_output
        impl#generic_params Default for Call#generic_params {
            fn default() -> Self {
                Call::Noop
            }
        }
    };

    (output, syn::parse2(struct_output).unwrap())
}

fn gen_param_input(generics: &Generics, bracketed: bool) -> TokenStream2 {
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
    } else if bracketed {
        quote!(<#(#gen_params),*>)
    } else {
        quote!(#(#gen_params),*)
    }
}
