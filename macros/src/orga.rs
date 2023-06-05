use crate::utils::{impl_item_attrs, path_to_ident};

use super::utils::is_attr_with_ident;
use darling::{ast, FromAttributes, FromDeriveInput, FromField, FromMeta, ToTokens};
use itertools::Itertools;
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
    #[darling(default)]
    channels: HashMap<Ident, ()>,
}

#[derive(Debug, FromMeta)]
struct OrgaImplAttrReceiver {
    #[darling(default)]
    channels: HashMap<Ident, ()>,
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
    channel: Option<HashMap<Ident, ()>>,
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
    channel: Option<Ident>,
}

impl OrgaSubStruct {
    fn ident(&self) -> Ident {
        if self.is_last {
            return self.ident_with_channel();
        }
        format_ident!("{}V{}", self.ident_with_channel(), self.version)
    }

    fn ident_with_channel(&self) -> Ident {
        if let Some(ref channel) = self.channel {
            format_ident!("{}{}", self.base_ident, channel)
        } else {
            self.base_ident.clone()
        }
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
        maybe_add("Describe", quote! { ::orga::describe::Describe });

        if self.is_last {
            // maybe_add("Call", quote! { ::orga::call::Call });
            maybe_add("Call", quote! { ::orga::call::FieldCall });
            maybe_add("Query", quote! { ::orga::query::FieldQuery });
            // maybe_add("Client", quote! { ::orga::client::Client });
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
            let prev_name = format!("{}V{}", self.ident_with_channel(), version - 1);
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
            let prev_name = format!("{}V{}", self.ident_with_channel(), version - 1);
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
            let versioned_ident = format_ident!("{}V{}", ident, self.version);
            tokens.extend(quote! {
               #vis type #versioned_ident #decl_generics = #ident #decl_generics;
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

    fn channels_iter(&self) -> impl Iterator<Item = Option<Ident>> + '_ {
        self.attrs
            .channels
            .iter()
            .map(|(ident, _)| Some(ident.clone()))
    }

    fn all_substructs(&self) -> impl Iterator<Item = OrgaSubStruct> + '_ {
        let channels: Vec<_> = if self.attrs.channels.is_empty() {
            vec![None]
        } else {
            self.channels_iter().collect()
        };

        (0..=self.attrs.version)
            .cartesian_product(channels)
            .map(move |(v, channel)| self.substruct_for_version_channel(v, channel))
    }

    fn substruct_for_version_channel(&self, version: u8, channel: Option<Ident>) -> OrgaSubStruct {
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
            .filter(|f| {
                f.channel.is_none()
                    || f.channel
                        .as_ref()
                        .unwrap()
                        .contains_key(&channel.as_ref().unwrap())
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
            channel,
        }
    }
}

impl ToTokens for OrgaMetaStruct {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let substructs = self.all_substructs();

        tokens.extend(quote! {
            #(#substructs)*
        })
    }
}

#[derive(Debug, Clone, FromAttributes)]
#[darling(attributes(orga))]
struct OrgaMethodAttr {
    channel: Option<HashMap<Ident, ()>>,
}

fn expand_item_impl(attr_args: AttributeArgs, item: ItemImpl) -> TokenStream {
    let attrs = OrgaImplAttrReceiver::from_list(&attr_args).unwrap();
    let channels = attrs
        .channels
        .iter()
        .map(|(ident, _)| ident.clone())
        .collect_vec();

    let mut tokens = TokenStream2::new();
    if channels.is_empty() {
        return quote! {
            #[::orga::call::call_block]
            #[::orga::query::query_block]
            #item
        }
        .into();
    }
    for channel in channels {
        let mut item = item.clone();
        if let box Type::Path(TypePath { path, .. }) = &mut item.self_ty {
            let generic_args = path.segments.last().unwrap().arguments.clone();
            let ident = path_to_ident(&path);
            let ident = format_ident!("{}{}", ident, channel);

            *path = parse_quote! {#ident #generic_args};
        }

        item.items = item
            .items
            .into_iter()
            .filter(|impl_item| {
                let attrs = impl_item_attrs(impl_item);
                let orga_method_attr = OrgaMethodAttr::from_attributes(&attrs)
                    .expect("Failed to parse orga method attribute");
                if let Some(channels) = &orga_method_attr.channel {
                    return channels.contains_key(&channel);
                }

                true
            })
            .collect();

        tokens.extend(quote! {
            #[::orga::orga]
            #item
        });
    }
    return tokens.into();
}

pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);
    if let Ok(item_impl) = syn::parse::<ItemImpl>(input.clone()) {
        return expand_item_impl(attr_args, item_impl);
    }
    if let Ok(_impl_item) = syn::parse::<ImplItem>(input.clone()) {
        return input;
    }
    let item = parse_macro_input!(input as DeriveInput);
    let metastruct = OrgaMetaStruct::new(attr_args, item);

    metastruct.into_token_stream().into()
}
