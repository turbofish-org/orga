use super::utils::is_attr_with_ident;
use darling::{ast, FromDeriveInput, FromField, FromMeta, ToTokens};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::*;

/// Arguments to the top-level orga macro invocation.
#[derive(Debug, FromMeta)]
struct OrgaAttrReceiver {
    #[darling(default)]
    version: u8,
    #[darling(default)]
    skip: HashMap<Ident, ()>,
    #[darling(default)]
    simple: bool,
}

/// Field-level data. This type is used in both the meta struct and the sub
/// structs. Attributes from other macros are passed through, including
/// attributes from other core macros like Call and State.
#[derive(Debug, FromField, Clone)]
#[darling(attributes(orga), forward_attrs)]
struct OrgaFieldReceiver {
    ident: Option<Ident>,
    vis: Visibility,
    attrs: Vec<Attribute>,
    ty: Type,
    version: Option<HashMap<Ident, ()>>,
}

/// Derive-style data about the top-level struct. Excludes attributes passed to
/// the orga attribute itself.
#[derive(FromDeriveInput, Debug, Clone)]
#[darling(attributes(orga), supports(struct_any), forward_attrs)]
struct OrgaInputReceiver {
    ident: Ident,
    generics: Generics,
    vis: Visibility,
    attrs: Vec<Attribute>,
    data: ast::Data<(), OrgaFieldReceiver>,
}

/// A sub struct that is generated for each version, containing only the fields
/// that should be present in that version.
struct OrgaSubStruct {
    base_ident: Ident,
    generics: Generics,
    vis: Visibility,
    attrs: Vec<Attribute>,
    data: ast::Data<(), OrgaFieldReceiver>,
    version: u8,
    is_last: bool,
    skip: HashMap<Ident, ()>,
    simple: bool,
}

impl OrgaSubStruct {
    fn ident(&self) -> Ident {
        format_ident!("{}V{}", self.base_ident, self.version)
    }

    fn all_attrs(&self) -> impl Iterator<Item = Attribute> {
        let mut derives = vec![];
        let mut maybe_add = |name: &str, full_path| {
            let name: Ident = format_ident!("{}", name);
            if !self.skip.contains_key(&name) {
                derives.push(quote! {#full_path})
            }
        };
        maybe_add("Default", quote! { Default});
        maybe_add("MigrateFrom", quote! { ::orga::migrate::MigrateFrom });
        maybe_add(
            "VersionedEncoding",
            quote! { ::orga::encoding::VersionedEncoding },
        );
        maybe_add("State", quote! { ::orga::state::State });
        maybe_add("Serialize", quote! { ::orga::serde::Serialize });

        if self.is_last {
            maybe_add("Call", quote! { ::orga::call::Call });
            maybe_add("Query", quote! { ::orga::query::Query });
            maybe_add("Client", quote! { ::orga::client::Client });
        }

        let mut attrs: Vec<Attribute> = vec![parse_quote! {#[derive(#(#derives),*)]}];

        attrs.push(self.state_attr());
        attrs.push(self.encoding_attr());
        if self.simple {
            attrs.push(self.migrate_from_attr());
            attrs.push(parse_quote! {#[derive(Clone)]})
        }

        attrs.into_iter().chain(self.attrs.clone().into_iter())
    }

    fn state_attr(&self) -> Attribute {
        let version = self.version;

        let maybe_prev = if self.version > 0 {
            let prev_name = format!("{}V{}", self.base_ident, version - 1);
            quote! {previous = #prev_name,}
        } else {
            quote! {}
        };

        let maybe_as_type = if self.simple {
            let as_type_name = quote! { "::orga::encoding::Adapter<Self>" };
            quote! {as_type = #as_type_name,}
        } else {
            quote! {}
        };

        parse_quote!(#[state(version = #version, #maybe_prev #maybe_as_type)])
    }

    fn encoding_attr(&self) -> Attribute {
        let version = self.version;

        let maybe_prev = if self.version > 0 {
            let prev_name = format!("{}V{}", self.base_ident, version - 1);
            quote! {previous = #prev_name,}
        } else {
            quote! {}
        };

        let maybe_as_type = if self.simple {
            let as_type_name = quote! { "::orga::encoding::Adapter<Self>" };
            quote! {as_type = #as_type_name,}
        } else {
            quote! {}
        };

        parse_quote!(#[encoding(version = #version, #maybe_prev #maybe_as_type)])
    }

    fn migrate_from_attr(&self) -> Attribute {
        parse_quote!(#[migrate_from(identity)])
    }
}

impl ToTokens for OrgaSubStruct {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Self {
            base_ident,
            generics,
            data,
            is_last,
            vis,
            ..
        } = self;
        let ident = self.ident();
        let attrs = self.all_attrs();
        let (imp, decl_generics, wher) = generics.split_for_impl();
        let body = data.clone().take_struct().unwrap();

        let fields = body.fields.iter().enumerate().map(|(i, f)| {
            let OrgaFieldReceiver {
                ty,
                vis,
                attrs,
                ident,
                ..
            } = &f;
            let field_ident = ident.as_ref().map_or(quote! {#i}, |ident| quote! {#ident});
            let attrs = attrs
                .iter()
                .filter(|attr| !(is_attr_with_ident(attr, "call") && !is_last))
                .map(|attr| quote! {#attr});
            quote! {
                #(#attrs)*
                #vis #field_ident: #ty,
            }
        });

        if self.simple {
            tokens.extend(quote! {
                impl #imp From<::orga::encoding::Adapter<#ident #decl_generics>> for #ident #decl_generics #wher {
                    fn from(adapter: ::orga::encoding::Adapter<#ident #decl_generics>) -> Self {
                        adapter.0
                    }
                }

                impl #imp From<&#ident #decl_generics> for ::orga::encoding::Adapter<#ident #decl_generics> #wher
                where #ident: Clone,
                 {
                    fn from(inner: &#ident #decl_generics) -> Self {
                        ::orga::encoding::Adapter(inner.clone())
                    }
                }

                impl #imp From<#ident #decl_generics> for ::orga::encoding::Adapter<#ident #decl_generics> #wher
                 {
                    fn from(inner: #ident #decl_generics) -> Self {
                        ::orga::encoding::Adapter(inner.clone())
                    }
                }
            });
        }

        tokens.extend(quote! {
            #(#attrs)*
            #vis struct #ident #imp #wher {
                #(#fields)*
            }
        });

        if *is_last {
            tokens.extend(quote! {
               #vis type #base_ident #decl_generics = #ident #decl_generics;
            });
        }
    }
}

/// A wrapper that holds the parsed input as well as orga attribute arguments.
struct OrgaMetaStruct {
    item: OrgaInputReceiver,
    attrs: OrgaAttrReceiver,
}

impl OrgaMetaStruct {
    fn new(args: Vec<NestedMeta>, input: DeriveInput) -> Self {
        let attrs = OrgaAttrReceiver::from_list(&args).unwrap();
        let item = OrgaInputReceiver::from_derive_input(&input).unwrap();

        Self { item, attrs }
    }

    fn versioned_structs(&self) -> impl Iterator<Item = OrgaSubStruct> + '_ {
        (0..=self.attrs.version).map(move |v| self.substruct_for_version(v))
    }

    fn substruct_for_version(&self, version: u8) -> OrgaSubStruct {
        let is_last = version == self.attrs.version;
        let item = self.item.clone();
        let style = item.clone().data.take_struct().unwrap().style;
        let fields: Vec<_> = item
            .clone()
            .data
            .take_struct()
            .unwrap()
            .fields
            .into_iter()
            .filter(|f| {
                f.version.is_none()
                    || f.version
                        .as_ref()
                        .unwrap()
                        .contains_key(&format_ident!("V{}", version))
            })
            .collect();
        let data = ast::Data::Struct(ast::Fields::new(style, fields));

        OrgaSubStruct {
            data,
            generics: item.generics,
            base_ident: item.ident,
            version,
            is_last,
            attrs: item.attrs,
            vis: item.vis,
            skip: self.attrs.skip.clone(),
            simple: self.attrs.simple,
        }
    }
}

impl ToTokens for OrgaMetaStruct {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let substructs = self.versioned_structs();

        tokens.extend(quote! {
            #(#substructs)*
        })
    }
}

pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);
    let item = parse_macro_input!(input as DeriveInput);
    let metastruct = OrgaMetaStruct::new(attr_args, item);

    metastruct.into_token_stream().into()
}
