use heck::{CamelCase, SnakeCase};
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::*;

use super::utils::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);
    let source = parse_parent();

    let name = &item.ident;
    let modname = Ident::new(
        format!("{}_client", name).to_snake_case().as_str(),
        Span::call_site(),
    );

    let field_adapters = create_field_adapters(&item);
    let client_impl = create_client_impl(&item, &modname);
    let client_struct = create_client_struct(&item, field_adapters.1, &source);

    let field_adapters = field_adapters.0;

    let output = quote! {
        pub mod #modname {
            use super::*;
            #client_struct
            #field_adapters
        }

        #client_impl
    };

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

    let mut generics_with_parent = generics_sanitized.clone();
    let parent_ty: GenericParam = syn::parse2(quote!(__Parent)).unwrap();
    generics_with_parent.params.push(parent_ty.clone());

    let generic_params = gen_param_input(&generics_sanitized, false);
    let generic_params_bracketed = gen_param_input(&generics_sanitized, true);
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);

    quote! {
        impl#generics_with_parent ::orga::client::Client<#parent_ty> for #name#generic_params_bracketed
        where
            #parent_ty: Clone + Send,
            #where_preds
        {
            type Client = #modname::Client<#generic_params #parent_ty>;

            fn create_client(parent: #parent_ty) -> Self::Client {
                #modname::Client::new(parent)
            }
        }
    }
}

fn create_client_struct(
    item: &DeriveInput,
    field_adapters: Vec<(&Field, ItemStruct)>,
    source: &File,
) -> TokenStream2 {
    let name = &item.ident;
    let generics = &item.generics;

    let parent_ty: GenericParam = syn::parse2(quote!(__Parent)).unwrap();
    let return_ty: GenericParam = syn::parse2(quote!(__Return)).unwrap();

    let mut generics_sanitized = generics.clone();
    generics_sanitized.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
        }
    });

    let mut generics_with_parent = generics_sanitized.clone();
    generics_with_parent.params.push(parent_ty.clone());

    let mut generics_with_return_and_parent = generics_sanitized.clone();
    generics_with_return_and_parent
        .params
        .push(return_ty.clone());
    generics_with_return_and_parent
        .params
        .push(parent_ty.clone());

    let generic_params = gen_param_input(generics, false);
    let generic_params_bracketed = gen_param_input(generics, true);
    let generic_params_bracketed_with_parent = gen_param_input(&generics_with_parent, true);
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);

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

    let field_clones = field_adapters
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

            let mut adapter_generics = adapter.generics.clone();
            adapter_generics.params.iter_mut().for_each(|g| {
                if let GenericParam::Type(ref mut t) = g {
                    t.default = None;
                    t.bounds = Default::default();
                }
            });

            quote!(#field_name: self.#field_name.clone())
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
            let field_ty = if let Type::Path(ref ty) = field_ty {
                let mut without_params = ty.clone();
                let params = without_params
                    .path
                    .segments
                    .last()
                    .unwrap()
                    .arguments
                    .clone();
                if let PathArguments::AngleBracketed(ref params) = params {
                    without_params.path.segments.last_mut().unwrap().arguments =
                        PathArguments::None;
                    quote!(#without_params::#params)
                } else {
                    quote!(#field_ty)
                }
            } else {
                quote!(#field_ty)
            };

            let adapter_name = &adapter.ident;

            quote!(#field_name: <#field_ty>::create_client(#adapter_name::new(parent.clone())))
        });

    let call_method_impls_and_adapters =
        relevant_methods(name, "call", source)
            .into_iter()
            .map(|(method, impl_item)| {
                let method_name = &method.sig.ident;

                let mut method_inputs = method.sig.inputs.clone();
                method_inputs
                    .iter_mut()
                    .for_each(|arg| {
                        if let FnArg::Receiver(ref mut arg) = arg {
                            arg.mutability = None;
                        }
                    });

                let arg_types: Vec<_> = method_inputs
                    .iter()
                    .filter_map(|arg| match arg {
                        FnArg::Typed(arg) => Some(arg.ty.clone()),
                        _ => None,
                    })
                    .map(|ty| quote!(#ty))
                    .collect();
                let unrolled_args: Vec<_> = (0..arg_types.len()).map(|i| {
                    let i = Literal::usize_unsuffixed(i);
                    quote!(cloned_args.#i)
                }).collect();
                let method_input_names = method_inputs
                    .iter()
                    .filter_map(|arg| match arg {
                        FnArg::Typed(arg) => Some(arg.pat.clone()),
                        _ => None,
                    })
                    .map(|pat| quote!(#pat));

                let adapter_name = Ident::new(
                    format!("Method{}Adapter", method_name.to_string().to_camel_case()).as_str(),
                    Span::call_site(),
                );
                let call_variant_name = Ident::new(
                    format!("Method{}", method_name.to_string().to_camel_case()).as_str(),
                    Span::call_site(),
                );

                let output_ty = match method.sig.output.clone() {
                    ReturnType::Default => quote!(()),
                    ReturnType::Type(_, mut ty) => {
                        replace_lifetimes(&mut ty, "'static");
                        quote!(#ty)
                    },
                };
                let method_output = quote!(
                    ::orga::client::CallChain<
                        <#output_ty as ::orga::client::Client<#adapter_name<#generic_params #output_ty, #parent_ty>>>::Client,
                        #adapter_name<#generic_params #output_ty, #parent_ty>,
                    >
                );

                let impl_preds = impl_item
                    .generics
                    .where_clause
                    .as_ref()
                    .map(|w| &w.predicates);

                let source = parse_parent();
                let call_enum = super::call::create_call_enum(&item, &source).1;
                let call_impl = super::call::create_call_impl(&item, &source, &call_enum).1;
                let call_preds = call_impl.generics.where_clause.as_ref().map(|w| &w.predicates);

                quote! {
                    pub struct #adapter_name#generics_with_return_and_parent
                    where
                        #parent_ty: Clone + Send,
                        #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
                    {
                        pub(super) parent: #parent_ty,
                        args: (#(#arg_types,)*),
                        _marker: std::marker::PhantomData<fn() -> (#name#generic_params_bracketed, __Return)>,
                    }

                    impl#generics_with_return_and_parent Clone for #adapter_name<#generic_params __Return, #parent_ty>
                    where
                        #parent_ty: Clone + Send,
                        #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
                        #call_preds
                    {
                        fn clone(&self) -> Self {
                            let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
                            let cloned_args = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
                            Self {
                                parent: self.parent.clone(),
                                args: cloned_args,
                                _marker: std::marker::PhantomData,
                            }
                        }
                    }

                    #[::orga::async_trait(?Send)]
                    impl#generics_with_return_and_parent ::orga::client::AsyncCall for #adapter_name<#generic_params __Return, #parent_ty>
                    where
                        #parent_ty: Clone + Send,
                        #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
                        __Return: ::orga::call::Call,
                        __Return::Call: Send + Sync,
                        #call_preds
                    {
                        type Call = <__Return as ::orga::call::Call>::Call;

                        async fn call(&self, call: Self::Call) -> ::orga::Result<()> {
                            let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
                            let cloned_args: (
                                #(#arg_types,)*
                            ) = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
                            let call_bytes = ::orga::encoding::Encode::encode(&call)?;
                            let parent_call = <#name#generic_params_bracketed as ::orga::call::Call>::Call::#call_variant_name(
                                #(#unrolled_args,)*
                                call_bytes
                            );
                            self.parent.call(parent_call).await
                        }
                    }

                    impl#generics_with_parent Client<#generic_params #parent_ty>
                    where
                        #parent_ty: Clone + Send,
                        #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
                        #where_preds
                        #impl_preds
                        #call_preds
                    {
                        pub fn #method_name(#method_inputs) -> #method_output {
                            let adapter = #adapter_name {
                                parent: self.parent.clone(),
                                args: (#(#method_input_names,)*),
                                _marker: std::marker::PhantomData,
                            };
                            let client = <#output_ty as ::orga::client::Client<#adapter_name<#generic_params _, #parent_ty>>>::create_client(adapter.clone());
                            ::orga::client::CallChain::new(client, adapter)
                        }
                    }
                }
            });

    let query_method_impls_and_adapters =
    relevant_methods(name, "query", source)
        .into_iter()
        .map(|(method, impl_item)| {
            let method_name = &method.sig.ident;

            let method_inputs = &method.sig.inputs;
            let arg_types: Vec<_> = method_inputs
                .iter()
                .filter_map(|arg| match arg {
                    FnArg::Typed(arg) => Some(arg.ty.clone()),
                    _ => None,
                })
                .map(|ty| quote!(#ty))
                .collect();
            let unrolled_args: Vec<_> = (0..arg_types.len()).map(|i| {
                let i = Literal::usize_unsuffixed(i);
                quote!(cloned_args.#i)
            }).collect();
            let method_input_names = method_inputs
                .iter()
                .filter_map(|arg| match arg {
                    FnArg::Typed(arg) => Some(arg.pat.clone()),
                    _ => None,
                })
                .map(|pat| quote!(#pat));

            let adapter_name = Ident::new(
                format!("Method{}Adapter", method_name.to_string().to_camel_case()).as_str(),
                Span::call_site(),
            );
            let query_variant_name = Ident::new(
                format!("Method{}", method_name.to_string().to_camel_case()).as_str(),
                Span::call_site(),
            );

            let output_ty = match method.sig.output.clone() {
                ReturnType::Default => quote!(()),
                ReturnType::Type(_, ty) => quote!(#ty),
            };
            let output_ty_with_explicit_lifetime = match method.sig.output.clone() {
                ReturnType::Default => quote!(()),
                ReturnType::Type(_, mut ty) => {
                    replace_lifetimes(&mut ty, "'orga_asyncquery_response");
                    quote!(#ty)
                },
            };
            let method_output = quote!(
                ::orga::client::QueryChain<
                    <#output_ty as ::orga::client::Client<#adapter_name<#generic_params #output_ty, #parent_ty>>>::Client,
                    #adapter_name<#generic_params #output_ty, #parent_ty>,
                >
            );

            let impl_preds = impl_item
                .generics
                .where_clause
                .as_ref()
                .map(|w| &w.predicates);

            let source = parse_parent();
            let query_enum = super::query::create_query_enum(&item, &source).1;
            let query_impl = super::query::create_query_impl(&item, &source, &query_enum).1;
            let query_preds = query_impl.generics.where_clause.as_ref().map(|w| &w.predicates);

            quote! {
                pub struct #adapter_name#generics_with_return_and_parent
                where
                    #parent_ty: Clone + Send,
                    #parent_ty: ::orga::client::AsyncQuery<Query = <#name#generic_params_bracketed as ::orga::query::Query>::Query>,
                    #parent_ty: for<'a> ::orga::client::AsyncQuery<Response<'a> = ::std::rc::Rc<#name#generic_params_bracketed>>,
                {
                    pub(super) parent: #parent_ty,
                    args: (#(#arg_types,)*),
                    _marker: std::marker::PhantomData<fn(#name#generic_params_bracketed, __Return)>,
                }

                impl#generics_with_return_and_parent Clone for #adapter_name<#generic_params __Return, #parent_ty>
                where
                    #parent_ty: Clone + Send,
                    #parent_ty: ::orga::client::AsyncQuery<Query = <#name#generic_params_bracketed as ::orga::query::Query>::Query>,
                    #parent_ty: for<'a> ::orga::client::AsyncQuery<Response<'a> = ::std::rc::Rc<#name#generic_params_bracketed>>,
                    #query_preds
                {
                    fn clone(&self) -> Self {
                        let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
                        let cloned_args = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
                        Self {
                            parent: self.parent.clone(),
                            args: cloned_args,
                            _marker: std::marker::PhantomData,
                        }
                    }
                }

                #[::orga::async_trait(?Send)]
                impl#generics_with_return_and_parent ::orga::client::AsyncQuery for #adapter_name<#generic_params __Return, #parent_ty>
                where
                    #parent_ty: Clone + Send,
                    #parent_ty: ::orga::client::AsyncQuery<Query = <#name#generic_params_bracketed as ::orga::query::Query>::Query>,
                    #parent_ty: for<'a> ::orga::client::AsyncQuery<Response<'a> = ::std::rc::Rc<#name#generic_params_bracketed>>,
                    __Return: ::orga::query::Query,
                    #query_preds
                {
                    type Query = <__Return as ::orga::query::Query>::Query;
                    type Response<'orga_asyncquery_response> = ::std::rc::Rc<#output_ty_with_explicit_lifetime>;

                    async fn query<F, R>(&self, query: Self::Query, mut check: F) -> ::orga::Result<R>
                    where
                        F: FnMut(Self::Response<'_>) -> ::orga::Result<R>
                    {
                        let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
                        let cloned_args: (
                            #(#arg_types,)*
                        ) = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
                        let query_bytes = ::orga::encoding::Encode::encode(&query)?;
                        let parent_query = <#name#generic_params_bracketed as ::orga::query::Query>::Query::#query_variant_name(
                            #(#unrolled_args,)*
                            query_bytes
                        );
                        ::orga::client::AsyncQuery::query(
                            &self.parent,
                            parent_query,
                            |s| {
                                let cloned_args: (
                                    #(#arg_types,)*
                                ) = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
                                check(::std::rc::Rc::new(s.#method_name(#(#unrolled_args,)*)))
                            },
                        ).await
                    }
                }

                impl#generics_with_parent Client<#generic_params #parent_ty>
                where
                    #parent_ty: Clone + Send,
                    #parent_ty: ::orga::client::AsyncQuery<Query = <#name#generic_params_bracketed as ::orga::query::Query>::Query>,
                    #parent_ty: for<'a> ::orga::client::AsyncQuery<Response<'a> = ::std::rc::Rc<#name#generic_params_bracketed>>,
                    #where_preds
                    #impl_preds
                    #query_preds
                {
                    pub fn #method_name(#method_inputs) -> #method_output {
                        let adapter = #adapter_name {
                            parent: self.parent.clone(),
                            args: (#(#method_input_names,)*),
                            _marker: std::marker::PhantomData,
                        };
                        let client = <#output_ty as ::orga::client::Client<#adapter_name<#generic_params _, #parent_ty>>>::create_client(adapter.clone());
                        ::orga::client::QueryChain::new(client, adapter)
                    }
                }
            }
        });

    quote! {
        #[must_use]
        pub struct Client#generics_with_parent
        where
            #parent_ty: Clone + Send,
            #where_preds
        {
            pub(super) parent: #parent_ty,
            #(#field_fields,)*
            __Marker: std::marker::PhantomData<fn(#generic_params)>,
        }

        impl#generics_with_parent Clone for Client#generic_params_bracketed_with_parent
        where
            #parent_ty: Clone + Send,
            #where_preds
        {
            fn clone(&self) -> Self {
                Self {
                    parent: self.parent.clone(),
                    #(#field_clones,)*
                    __Marker: std::marker::PhantomData,
                }
            }
        }

        impl#generics_with_parent Client#generic_params_bracketed_with_parent
        where
            #parent_ty: Clone + Send,
            #where_preds
        {
            pub fn new(parent: #parent_ty) -> Self {
                use ::orga::client::Client as _;
                Client {
                    #(#field_constructors,)*
                    parent,
                    __Marker: std::marker::PhantomData,
                }
            }
        }

        #(#call_method_impls_and_adapters)*
        #(#query_method_impls_and_adapters)*
    }
}

fn create_field_adapters(item: &DeriveInput) -> (TokenStream2, Vec<(&Field, ItemStruct)>) {
    let fields: Vec<_> = struct_fields(&item)
        .filter(|field| matches!(field.vis, Visibility::Public(_)))
        .collect();

    let item_name = &item.ident;
    let item_generics = &item.generics;

    let parent_ty: GenericParam = syn::parse2(quote!(__Parent)).unwrap();
    let return_ty: GenericParam = syn::parse2(quote!(__Return)).unwrap();

    let generic_params_bracketed = gen_param_input(&item_generics, true);

    let mut generics_sanitized = item_generics.clone();
    generics_sanitized.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
        }
    });
    generics_sanitized.params.push(parent_ty.clone());

    let mut generics_with_parent = item_generics.clone();
    generics_with_parent.params.push(parent_ty.clone());

    let mut generics_with_return_and_parent = item_generics.clone();
    generics_with_return_and_parent
        .params
        .push(return_ty.clone());
    generics_with_return_and_parent
        .params
        .push(parent_ty.clone());

    let mut generics_sanitized_with_return = generics_sanitized.clone();
    generics_sanitized_with_return.params.push(return_ty);

    let generic_params_bracketed_with_parent = gen_param_input(&generics_with_parent, true);

    let item_ty = quote!(#item_name#generic_params_bracketed);

    let adapters: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let field_name = &field.ident;

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

            let async_call_def = if field.attrs.iter().any(|a| a.path.is_ident("call")) {
                quote! {
                    #[::orga::async_trait(?Send)]
                    impl#generics_with_parent ::orga::client::AsyncCall for #struct_name#generic_params_bracketed_with_parent
                    where
                        #parent_client_ty: Clone + Send,
                        #parent_client_ty: ::orga::client::AsyncCall<Call = <#item_ty as ::orga::call::Call>::Call>,
                    {
                        type Call = <#field_ty as ::orga::call::Call>::Call;

                        async fn call(&self, call: Self::Call) -> ::orga::Result<()> {
                            // assumes that the call has a tuple variant called "Field" +
                            // the camel-cased name as the field
                            let subcall_bytes = ::orga::encoding::Encode::encode(&call)?; // TODO: error handling
                            let subcall = <#item_ty as ::orga::call::Call>::Call::#variant_name(subcall_bytes);
                            self.parent.call(subcall).await
                        }
                    }

                }
            } else {
                quote! {}
            };

            let struct_def = quote! {
                #[derive(Clone)]
                pub struct #struct_name#generics_with_parent
                where
                    #parent_client_ty: Clone + Send,
                {
                    pub(super) parent: #parent_client_ty,
                    marker: ::std::marker::PhantomData<fn() -> #item_ty>,
                }
            };

            let output = quote! {
                #struct_def
                impl#generics_with_parent #struct_name#generic_params_bracketed_with_parent
                where
                    #parent_client_ty: Clone + Send,
                {
                    pub fn new(parent: #parent_client_ty) -> Self {
                        Self { parent, marker: ::std::marker::PhantomData }
                    }
                }

                #async_call_def

                #[::orga::async_trait(?Send)]
                impl#generics_with_parent ::orga::client::AsyncQuery for #struct_name#generic_params_bracketed_with_parent
                where
                    #parent_client_ty: Clone + Send,
                    #parent_client_ty: ::orga::client::AsyncQuery<Query = <#item_ty as ::orga::query::Query>::Query>,
                    #parent_client_ty: for<'a> ::orga::client::AsyncQuery<Response<'a> = ::std::rc::Rc<#item_ty>>,
                {
                    type Query = <#field_ty as ::orga::query::Query>::Query;
                    type Response<'a> = ::std::rc::Rc<#field_ty>;

                    async fn query<F, R>(&self, query: Self::Query, mut check: F) -> ::orga::Result<R>
                    where
                        F: FnMut(Self::Response<'_>) -> ::orga::Result<R>
                    {
                        // assumes that the query has a tuple variant called "Field" +
                        // the camel-cased name as the field
                        let subcall = <#item_ty as ::orga::query::Query>::Query::#variant_name(query);
                        ::orga::client::AsyncQuery::query(&self.parent, subcall, |s| check(std::rc::Rc::new(std::rc::Rc::try_unwrap(s).map_err(|_| ()).unwrap().#field_name))).await
                    }
                }
            };

            (output, struct_def)
        })
        .collect();
    let adapter_outputs = adapters.clone().into_iter().map(|a| a.0);
    let adapter_items: Vec<_> = fields
        .iter()
        .map(|f| *f)
        .zip(adapters.into_iter().map(|a| syn::parse2(a.1).unwrap()))
        .collect();

    let output = quote!(#(#adapter_outputs)*);

    (output, adapter_items)
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

fn replace_lifetimes(ty: &mut Type, name: &str) {
    match ty {
        Type::Path(path) => {
            if let Some(last_segment) = path.path.segments.last_mut() {
                if let PathArguments::AngleBracketed(args) = &mut last_segment.arguments {
                    args.args.iter_mut().for_each(|arg| match arg {
                        GenericArgument::Lifetime(ref mut ty) => {
                            *ty = Lifetime::new(name, Span::call_site());
                        }
                        GenericArgument::Type(ty) => {
                            replace_lifetimes(ty, name);
                        }
                        _ => {}
                    });
                }
            }
        }
        Type::Reference(ref mut ref_) => {
            ref_.lifetime = Some(Lifetime::new(name, Span::call_site()));
            replace_lifetimes(&mut ref_.elem, name);
        }
        Type::Tuple(ref mut tuple) => tuple
            .elems
            .iter_mut()
            .for_each(|t| replace_lifetimes(t, name)),
        _ => {}
    }
}
