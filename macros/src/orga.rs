use super::utils::is_attr_with_ident;
use darling::{ast, export::NestedMeta, FromDeriveInput, FromField, FromMeta, ToTokens};
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::{collections::HashMap, ops::RangeInclusive};
use syn::*;

#[derive(Debug, Clone)]
struct VersionSpec {
    range: RangeInclusive<u8>,
}

impl Default for VersionSpec {
    fn default() -> Self {
        Self { range: 0..=0 }
    }
}

impl FromMeta for VersionSpec {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        if let Expr::Range(range) = expr {
            let (
                Expr::Lit(ExprLit {
                    lit: Lit::Int(start),
                    ..
                }),
                Expr::Lit(ExprLit {
                    lit: Lit::Int(end), ..
                }),
            ) = (
                *range
                    .start
                    .as_ref()
                    .expect("Start of version range must be specified")
                    .clone(),
                *range
                    .end
                    .as_ref()
                    .expect("End of version range must be specified")
                    .clone(),
            )
            else {
                return Err(darling::Error::custom(
                    "Version must be integer or inclusive range",
                ));
            };

            let start: u8 = start.base10_parse().unwrap_or_default();
            let end: u8 = end
                .base10_parse()
                .expect("Version range end must be integer");
            if matches!(range.limits, RangeLimits::HalfOpen(_)) {
                return Err(darling::Error::custom("Version range must be inclusive"));
            }

            return Ok(Self { range: start..=end });
        }
        if let Expr::Lit(ExprLit {
            lit: Lit::Int(value),
            ..
        }) = expr
        {
            let version: u8 = value
                .base10_parse()
                .expect("Version must be integer or inclusive range");

            return Ok(Self { range: 0..=version });
        }

        return Err(darling::Error::custom(
            "Version must be integer or inclusive range",
        ));
    }
}

/// Arguments to the top-level orga macro invocation.
#[derive(Debug, FromMeta, Clone)]
struct OrgaAttrReceiver {
    #[darling(default)]
    version: VersionSpec,
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
#[derive(Clone)]
struct OrgaSubStruct {
    base_ident: Ident,
    generics: Generics,
    vis: Visibility,
    attrs: Vec<Attribute>,
    data: ast::Data<(), OrgaFieldReceiver>,
    version: u8,
    version_start: u8,
    is_last: bool,
    skip: HashMap<Ident, ()>,
    simple: bool,
    channel: Option<Ident>,
    prev_generics: Option<Generics>,
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
        maybe_add(
            "VersionedEncoding",
            quote! { ::orga::encoding::VersionedEncoding },
        );
        maybe_add("State", quote! { ::orga::state::State });
        maybe_add("Serialize", quote! { ::orga::serde::Serialize });
        maybe_add("Migrate", quote! { ::orga::migrate::Migrate });

        if self.is_last {
            maybe_add("Call", quote! { ::orga::call::FieldCall });
            maybe_add("Query", quote! { ::orga::query::FieldQuery });
            maybe_add("Describe", quote! { ::orga::describe::Describe });
        }

        let mut attrs: Vec<Attribute> = vec![parse_quote! {#[derive(#(#derives),*)]}];

        attrs.push(self.state_attr());
        attrs.push(self.encoding_attr());

        let migrate_ident: Ident = format_ident!("Migrate");
        if !self.simple && !self.skip.contains_key(&migrate_ident) {
            attrs.push(self.migrate_attr());
        }

        if self.simple {
            attrs.push(parse_quote! {#[derive(Clone)]});
        }

        attrs.into_iter().chain(self.attrs.clone().into_iter())
    }

    fn state_attr(&self) -> Attribute {
        let version = self.version;

        let maybe_prev = if self.version > self.version_start {
            let prev_ty_generics = self
                .prev_generics
                .as_ref()
                .map(|g| g.split_for_impl().1.to_token_stream())
                .unwrap_or_default();
            let prev_name = format!(
                "{}V{}{}",
                self.ident_with_channel(),
                version - 1,
                prev_ty_generics.to_string(),
            );
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

        let maybe_prev = if self.version > self.version_start {
            let prev_ty_generics = self
                .prev_generics
                .as_ref()
                .map(|g| g.split_for_impl().1.to_token_stream())
                .unwrap_or_default();
            let prev_name = format!(
                "{}V{}{}",
                self.ident_with_channel(),
                version - 1,
                prev_ty_generics.to_string(),
            );
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

    fn migrate_attr(&self) -> Attribute {
        let version = self.version;

        let maybe_prev = if self.version > self.version_start {
            let prev_ty_generics = self
                .prev_generics
                .as_ref()
                .map(|g| g.split_for_impl().1.to_token_stream())
                .unwrap_or_default();
            let prev_name = format!(
                "{}V{}{}",
                self.ident_with_channel(),
                version - 1,
                prev_ty_generics.to_string(),
            );
            quote! {previous = #prev_name,}
        } else {
            quote! {}
        };

        // TODO
        // let maybe_as_type = if self.simple {
        //     let as_type_name = quote! { "::orga::encoding::Adapter<Self>" };
        //     quote! {as_type = #as_type_name,}
        // } else {
        //     quote! {}
        // };

        parse_quote!(#[migrate(version = #version, #maybe_prev)])
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
            let doctext = format!("Latest version of [{}]", self.base_ident);
            tokens.extend(quote! {
                #[doc = #doctext]
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

        let mut substructs = vec![];
        for channel in channels.iter() {
            let mut prev = None;
            let attrs = self.attrs.clone();
            for version in attrs.version.range {
                let substruct = self.substruct_for_version_channel(version, channel.clone(), &prev);
                substructs.push(substruct.clone());
                if version < *self.attrs.version.range.end() {
                    prev.replace(substruct);
                }
            }
        }

        substructs.into_iter()
    }

    fn substruct_for_version_channel(
        &self,
        version: u8,
        channel: Option<Ident>,
        maybe_prev: &Option<OrgaSubStruct>,
    ) -> OrgaSubStruct {
        let is_last = version == *self.attrs.version.range.end();
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
            version_start: *self.attrs.version.range.start(),
            is_last,
            attrs: item.attrs,
            vis: item.vis,
            skip: self.attrs.skip.clone(),
            simple: self.attrs.simple,
            channel,
            prev_generics: maybe_prev.as_ref().map(|prev| prev.generics.clone()),
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

fn expand_item_impl(attr_args: Vec<NestedMeta>, item: ItemImpl) -> TokenStream {
    let attrs = OrgaImplAttrReceiver::from_list(&attr_args).unwrap();
    let channels = attrs
        .channels
        .iter()
        .map(|(ident, _)| ident.clone())
        .collect_vec();

    if channels.is_empty() {
        quote! {
            #[::orga::call::call_block]
            #[::orga::query::query_block]
            #item
        }
    } else {
        quote! {

            #[::orga::channels(#(#channels),*)]
            #[::orga::call::call_block]
            #[::orga::query::query_block]
            #item
        }
    }
    .into()
}

pub fn orga(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = NestedMeta::parse_meta_list(args.into()).unwrap();
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
