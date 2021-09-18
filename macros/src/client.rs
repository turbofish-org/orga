use heck::{SnakeCase, CamelCase};
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let name = &item.ident;
    let modname = Ident::new(
        format!("{}_client", name).to_snake_case().as_str(),
        Span::call_site(),
    );

    let field_adapters = create_field_adapters(&item);
    let client_impl = create_client_impl(&item, &modname);
    let client_struct = create_client_struct(&item, field_adapters.1);

    let field_adapters = field_adapters.0;

    let output = quote! {
        pub mod #modname {
            use super::*;
            #client_struct
            #field_adapters
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

fn create_client_struct(item: &DeriveInput, field_adapters: Vec<(&Field, ItemStruct)>) -> TokenStream2 {
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

    let field_fields = field_adapters
        .iter()
        .enumerate()
        .map(|(i, (field, adapter))| {
            let field_name = field.ident.as_ref().map_or_else(
                || {
                    let i = Literal::usize_unsuffixed(i);
                    quote!(#i)
                },
                |f| quote!(#f),
            );
            let field_ty = &field.ty;

            let adapter_name = &adapter.ident;
            let mut adapter_generics = adapter.generics.clone();
            adapter_generics.params.iter_mut().for_each(|g| {
                if let GenericParam::Type(ref mut t) = g {
                    t.default = None;
                    t.bounds = Default::default();
                }
            });

            quote!(pub #field_name: <#field_ty as ::orga::client::Client<#adapter_name#adapter_generics>>::Client)
        });

    let field_constructors = field_adapters
        .iter()
        .enumerate()
        .map(|(i, (field, adapter))| {
            let field_name = field.ident.as_ref().map_or_else(
                || {
                    let i = Literal::usize_unsuffixed(i);
                    quote!(#i)
                },
                |f| quote!(#f),
            );
            let field_ty = &field.ty;

            let adapter_name = &adapter.ident;

            quote!(#field_name: #field_ty::create_client(#adapter_name::new(parent.clone())))
        });

    quote! {
        #[derive(Clone)]
        pub struct Client<#generic_params #parent_ty: Clone> {
            pub(super) parent: #parent_ty,
            _marker: std::marker::PhantomData<#name#generics_sanitized>,
            #(#field_fields,)*
        }

        impl<#parent_ty> Client<#parent_ty>
        where
            #parent_ty: Clone,
            #where_preds
        {
            pub fn new(parent: #parent_ty) -> Self {
                use ::orga::client::Client as _;
                Client {
                    _marker: std::marker::PhantomData,
                    #(#field_constructors,)*
                    parent,
                }
            }
        }
    }
}

fn create_field_adapters(item: &DeriveInput) -> (TokenStream2, Vec<(&Field, ItemStruct)>) {
    let fields: Vec<_> = struct_fields(&item).filter(|field| {
        matches!(field.vis, Visibility::Public(_))
    }).collect();

    let item_name = &item.ident;
    let item_generics = &item.generics;
    let item_ty = quote!(#item_name#item_generics);

    let adapters: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| create_field_adapter(&item_ty, f, i))
        .collect();
    let adapter_outputs = adapters.clone().into_iter().map(|a| a.0);
    let adapter_items: Vec<_> = fields
        .iter()
        .map(|f| *f)
        .zip(adapters.into_iter().map(|a| a.1))
        .collect();
    
    let output = quote!(#(#adapter_outputs)*);

    (output, adapter_items)
}

fn create_field_adapter(parent_ty: &TokenStream2, field: &Field, i: usize) -> (TokenStream2, ItemStruct) {
    let struct_name = field.ident.as_ref().map_or(
        Ident::new(format!("Field{}Adapter", i).as_str(), Span::call_site()),
        |f| {
            Ident::new(
                format!("Field{}Adapter", f.to_string().to_camel_case()).as_str(),
                Span::call_site(),
            )
        },
    );
    let variant_name = field.ident.as_ref().map_or(
        Ident::new(format!("Field{}", i).as_str(), Span::call_site()),
        |f| {
            Ident::new(
                format!("Field{}", f.to_string().to_camel_case()).as_str(),
                Span::call_site(),
            )
        },
    );
    let field_ty = &field.ty;
    let parent_client_ty: GenericParam = syn::parse2(quote!(__Parent)).unwrap();

    let struct_def = quote! {
        #[derive(Clone)]
        pub struct #struct_name<#parent_client_ty: Clone> {
            pub(super) parent: #parent_client_ty,
        }
    };

    let adapter_struct: ItemStruct = syn::parse2(struct_def.clone()).unwrap();
    let mut adapter_generics = adapter_struct.generics.clone();
    adapter_generics.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
        }
    });

    let output = quote! {
        #struct_def
        impl#adapter_generics #struct_name<#parent_client_ty> {
            pub fn new(parent: #parent_client_ty) -> Self {
                Self {
                    parent,
                }
            }
        }
        impl#adapter_generics ::orga::call::Call for #struct_name<#parent_client_ty>
        where
            #parent_client_ty: ::orga::call::Call<Call = <#parent_ty as ::orga::call::Call>::Call>,
        {
            type Call = <#field_ty as ::orga::call::Call>::Call;

            fn call(&mut self, call: Self::Call) -> Result<()> {
                // assumes that the call has a tuple variant called "Field" +
                // the camel-cased name as the field, with a single element
                // which is the Call type of the field (e.g. as generated by the
                // Call derive macro)
                self.parent.call(<#parent_ty as ::orga::call::Call>::Call::#variant_name(call))
            }
        }
    };

    (output, adapter_struct)
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

fn struct_fields(item: &DeriveInput) -> impl Iterator<Item = &Field> {
    let data = match item.data {
        Data::Struct(ref data) => data,
        Data::Enum(ref _data) => todo!("#[derive(Client)] does not yet support enums"),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    match data.fields {
        Fields::Named(ref fields) => fields.named.iter(),
        Fields::Unnamed(ref fields) => fields.unnamed.iter(),
        Fields::Unit => panic!("Unit structs are not supported"),
    }
}
