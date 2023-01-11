use std::collections::HashMap;

use darling::{ast, FromDeriveInput, FromField, FromMeta, ToTokens};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::*;
#[derive(Debug, FromMeta)]
struct OrgaAttrReceiver {
    #[darling(default)]
    version: u8,
    #[darling(default)]
    skip: HashMap<Ident, ()>,
}

impl OrgaAttrReceiver {}

#[derive(Debug, FromField, Clone)]
#[darling(attributes(orga), forward_attrs)]
struct OrgaFieldReceiver {
    ident: Option<Ident>,
    vis: Visibility,
    attrs: Vec<Attribute>,
    ty: Type,
    version: Option<HashMap<Ident, ()>>,
}

impl ToTokens for OrgaFieldReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        tokens.extend(quote! {});
    }
}

#[derive(FromDeriveInput, Debug, Clone)]
#[darling(attributes(orga), supports(struct_any), forward_attrs)]
struct OrgaInputReceiver {
    ident: Ident,
    generics: Generics,
    vis: Visibility,
    attrs: Vec<Attribute>,
    data: ast::Data<(), OrgaFieldReceiver>,
}

impl OrgaInputReceiver {
    fn filter_for_version(
        &self,
        version: u8,
        is_last: bool,
        skip: HashMap<Ident, ()>,
    ) -> OrgaSubStruct {
        let res = self.clone();
        let style = self.clone().data.take_struct().unwrap().style;
        let fields: Vec<_> = self
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
            .map(|field| field)
            .collect();
        let data = ast::Data::Struct(ast::Fields::new(style, fields));
        OrgaSubStruct {
            data,
            generics: res.generics,
            base_ident: res.ident,
            version,
            is_last,
            attrs: res.attrs,
            vis: res.vis,
            skip,
        }
    }
}

struct OrgaSubStruct {
    base_ident: Ident,
    generics: Generics,
    vis: Visibility,
    attrs: Vec<Attribute>,
    data: ast::Data<(), OrgaFieldReceiver>,
    version: u8,
    is_last: bool,
    skip: HashMap<Ident, ()>,
}

impl OrgaSubStruct {
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
        maybe_add("Encode", quote! { ::orga::encoding::VersionedEncode });
        maybe_add("Decode", quote! { ::orga::encoding::VersionedDecode });
        maybe_add("State", quote! { ::orga::state::State });

        if self.is_last {
            maybe_add("Call", quote! { ::orga::call::Call });
            maybe_add("Query", quote! { ::orga::query::Query });
        }

        let mut attrs: Vec<Attribute> = vec![parse_quote! {#[derive(#(#derives),*)]}];

        attrs.push(self.state_attr());

        attrs.into_iter().chain(self.attrs.clone().into_iter())
    }

    fn state_attr(&self) -> Attribute {
        let version = self.version;
        if self.version > 0 {
            let prev_name = format!("{}V{}", self.base_ident, version - 1);
            parse_quote!(#[state(version = #version, previous = #prev_name)])
        } else {
            parse_quote!(#[state(version = #version)])
        }
    }
}

impl ToTokens for OrgaSubStruct {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let Self {
            base_ident,
            generics,
            data,
            version,
            is_last,
            vis,
            ..
        } = self;

        let attrs = self.all_attrs();
        let (imp, decl_generics, wher) = generics.split_for_impl();
        let ident = format_ident!("{}V{}", base_ident, version);
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
            let attrs = attrs.iter().map(|attr| quote! {#attr});
            quote! {
                #(#attrs)*
                #vis #field_ident: #ty,
            }
        });

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
        (0..=self.attrs.version).map(move |v| {
            self.item
                .filter_for_version(v, v == self.attrs.version, self.attrs.skip.clone())
        })
    }
}

impl ToTokens for OrgaMetaStruct {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let item = &self.item;
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
