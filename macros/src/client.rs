use heck::SnakeCase;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let name = &item.ident;
    let modname = Ident::new(
        format!("{}_client", name).to_snake_case().as_str(),
        Span::call_site(),
    );

    let client_impl = create_client_impl(&item, &modname);
    let client_struct = create_client_struct(&item);

    let output = quote! {
        pub mod #modname {
            use super::*;
            #client_struct
        }

        #client_impl
    };

    println!("{}\n\n\n\n", &output);
    
    output.into()
}

fn create_client_impl(item: &DeriveInput, modname: &Ident) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;

    let mut generics_sanitized = generics.clone();
    generics_sanitized.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
        }
    });
    let parent_ty: GenericParam = syn::parse2(quote!(__Parent)).unwrap();
    generics_sanitized.params.push(parent_ty.clone());

    let generic_params = gen_param_input(generics, true);
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);

    quote! {
        impl#generics_sanitized ::orga::client::Client<#parent_ty> for #name#generic_params
        where
            #parent_ty: Clone,
            #where_preds
        {
            type Client = #modname::Client<#parent_ty>;

            fn create_client(parent: #parent_ty) -> Self::Client {
                #modname::Client::new(parent)
            }
        }
    }
}

fn create_client_struct(item: &DeriveInput) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;
    let mut generics_sanitized = generics.clone();
    generics_sanitized.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
            t.bounds = Default::default();
        }
    });
    let generic_params = gen_param_input(generics, false);
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);

    let parent_ty: GenericParam = syn::parse2(quote!(__Parent)).unwrap();

    quote! {
        #[derive(Clone)]
        pub struct Client<#generic_params #parent_ty: Clone> {
            parent: #parent_ty,
            _marker: std::marker::PhantomData<#name#generics_sanitized>,
        }

        impl<#parent_ty> Client<#parent_ty>
        where
            #parent_ty: Clone,
            #where_preds
        {
            pub fn new(parent: #parent_ty) -> Self {
                Client {
                    parent,
                    _marker: std::marker::PhantomData,
                }
            }

            pub fn foo(&self) -> u32 {
                123
            }
        }
    }
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
        quote!(#(#gen_params,)*)
    }
}
