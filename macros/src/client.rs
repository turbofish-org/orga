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

    println!("\n\n\n\n\n{}\n\n\n", &output);

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

    let generic_params = gen_param_input(generics, false);
    let generic_params_bracketed = gen_param_input(generics, true);
    let where_preds = item.generics.where_clause.as_ref().map(|w| &w.predicates);

    quote! {
        impl#generics_sanitized ::orga::client::Client<#parent_ty> for #name#generic_params_bracketed
        where
            #parent_ty: Clone,
            #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
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

    let mut generics_sanitized = generics.clone();
    generics_sanitized.params.iter_mut().for_each(|g| {
        if let GenericParam::Type(ref mut t) = g {
            t.default = None;
        }
    });
    generics_sanitized.params.push(parent_ty.clone());

    let mut generics_sanitized_with_return = generics_sanitized.clone();
    generics_sanitized_with_return.params.push(syn::parse_quote!(__Return));

    let generic_params = gen_param_input(generics, false);
    let generic_params_bracketed = gen_param_input(generics, true);
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

    let method_impls_and_adapters =
        relevant_methods(name, "call", source)
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
                    quote!(self.args.#i)
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
                        add_static_lifetimes(&mut ty);
                        quote!(#ty)
                    },
                };
                let method_output = quote!(
                    <#output_ty as ::orga::client::Client<#adapter_name<#generic_params #output_ty, #parent_ty>>>::Client
                );

                let impl_preds = impl_item
                    .generics
                    .where_clause
                    .as_ref()
                    .map(|w| &w.predicates);

                quote! {
                    #[derive(Clone)]
                    pub struct #adapter_name<#generic_params __Return, #parent_ty: Clone> {
                        pub(super) parent: #parent_ty,
                        args: (#(#arg_types,)*),
                        _marker: std::marker::PhantomData<(#name#generic_params_bracketed, __Return)>,
                    }

                    impl#generics_sanitized_with_return ::orga::client::AsyncCall for #adapter_name<#generic_params __Return, #parent_ty>
                    where
                        #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
                    {
                        type Call = <__Return as ::orga::call::Call>::Call;
                        type Future = #parent_ty::Future;

                        fn call(&mut self, call: Self::Call) -> Self::Future {
                            let call_bytes = ::ed::Encode::encode(&call).unwrap(); // TODO: is this always infallible?
                            let parent_call = <#name as ::orga::call::Call>::Call::#call_variant_name(
                                #(#unrolled_args,)*
                                call_bytes
                            );
                            self.parent.call(parent_call)
                        }
                    }

                    impl#generics_sanitized Client<#generic_params #parent_ty>
                    where
                        #parent_ty: Clone,
                        #parent_ty: ::orga::client::AsyncCall<Call = <#name#generic_params_bracketed as ::orga::call::Call>::Call>,
                        #where_preds
                        #impl_preds
                    {
                        pub fn #method_name(#method_inputs) -> #method_output {
                            let adapter = #adapter_name {
                                parent: self.parent.clone(),
                                args: (#(#method_input_names,)*),
                                _marker: std::marker::PhantomData,
                            };
                            <#output_ty as ::orga::client::Client<#adapter_name<#generic_params _, #parent_ty>>>::create_client(adapter)
                        }
                    }
                }
            });

    quote! {
        #[must_use]
        #[derive(Clone)]
        pub struct Client<#generic_params #parent_ty: Clone>
        where
            #parent_ty: ::orga::client::AsyncCall,
            #where_preds
        {
            pub(super) parent: #parent_ty,
            _marker: std::marker::PhantomData<#name#generic_params_bracketed>,
            #(#field_fields,)*
            fut: Option<std::pin::Pin<Box<#parent_ty::Future>>>
        }

        impl#generics_sanitized Client<#generic_params #parent_ty>
        where
            #parent_ty: Clone,
            #parent_ty: ::orga::client::AsyncCall,
            #where_preds
        {
            pub fn new(parent: #parent_ty) -> Self {
                use ::orga::client::Client as _;
                Client {
                    _marker: std::marker::PhantomData,
                    #(#field_constructors,)*
                    parent,
                    fut: None,
                }
            }
        }

        impl#generics_sanitized std::future::Future for Client<#generic_params #parent_ty>
        where
            #parent_ty: Clone,
            #parent_ty: ::orga::client::AsyncCall,
            #parent_ty::Call: Default,
            #where_preds
        {
            type Output = Result<()>;

            fn poll(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                unsafe {
                    let this = self.get_unchecked_mut();

                    if this.fut.is_none() {
                        // TODO: is this ok? we're creating the future then moving it
                        let res = this.parent.call(Default::default());
                        this.fut = Some(Box::pin(res));
                    }

                    let fut = this.fut.as_mut().unwrap().as_mut();
                    fut.poll(cx)
                }
            }
        }

        #(#method_impls_and_adapters)*
    }
}

fn create_field_adapters(item: &DeriveInput) -> (TokenStream2, Vec<(&Field, ItemStruct)>) {
    let fields: Vec<_> = struct_fields(&item)
        .filter(|field| matches!(field.vis, Visibility::Public(_)))
        .collect();

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

fn create_field_adapter(
    parent_ty: &TokenStream2,
    field: &Field,
    i: usize,
) -> (TokenStream2, ItemStruct) {
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

        impl#adapter_generics ::orga::client::AsyncCall for #struct_name<#parent_client_ty>
        where
            #parent_client_ty: ::orga::client::AsyncCall<Call = <#parent_ty as ::orga::call::Call>::Call>,
        {
            type Call = <#field_ty as ::orga::call::Call>::Call;
            type Future = #parent_client_ty::Future;

            fn call(&mut self, call: Self::Call) -> Self::Future {
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

fn add_static_lifetimes(ty: &mut Type) {
    match ty {
        Type::Path(path) => {
            if let Some(last_segment) = path.path.segments.last_mut() {
                if let PathArguments::AngleBracketed(args) = &mut last_segment.arguments {
                    args.args.iter_mut().for_each(|arg| {
                        if let GenericArgument::Type(ty) = arg {
                            add_static_lifetimes(ty);
                        }
                    });
                }
            }
        }
        Type::Reference(ref mut ref_) => {
            ref_.lifetime = Some(Lifetime::new("'static", Span::call_site()));
            add_static_lifetimes(&mut ref_.elem);
        }
        _ => {}
    }
}
