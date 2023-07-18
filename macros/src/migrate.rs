use darling::{
    ast,
    usage::{GenericsExt, Options, Purpose, UsesTypeParams},
    FromDeriveInput,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::*;

use crate::state::StateFieldReceiver;

#[derive(FromDeriveInput)]
#[darling(attributes(migrate), supports(struct_any))]
struct MigrateInputReceiver {
    ident: Ident,
    generics: Generics,
    data: ast::Data<(), StateFieldReceiver>,

    #[darling(default)]
    identity: bool,
    #[darling(default)]
    version: u8,
    previous: Option<Path>,
}

impl ToTokens for MigrateInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let MigrateInputReceiver {
            ident,
            generics,
            data,
            identity,
            version,
            previous,
        } = self;

        let (imp, ty, wher) = generics.split_for_impl();

        if *identity {
            return tokens.extend(quote! {
                impl #imp ::orga::migrate::Migrate for #ident #ty
                #wher
                {}
            });
        }

        let fields = data.as_ref().take_struct().unwrap().fields;

        let field_migrations = fields.iter().enumerate().map(|(i, f)| {
            let field_ident = f.ident.as_ref().map(|v| quote!(#v)).unwrap_or_else(|| {
                let i = syn::Index::from(i);
                quote!(#i)
            });

            if f.skip {
                quote! { #field_ident: Default::default(), }
            } else {
                quote! { #field_ident: ::orga::migrate::Migrate::migrate(
                    <Self as ::orga::state::State>::field_keyop(stringify!(#field_ident)).unwrap_or(::orga::describe::KeyOp::Append(vec![])).apply(&src),
                    <Self as ::orga::state::State>::field_keyop(stringify!(#field_ident)).unwrap_or(::orga::describe::KeyOp::Append(vec![])).apply(&dest),
                    &mut bytes,
                )?, }
            }
        });

        let search_options: Options = Purpose::BoundImpl.into();
        let decl_tp = generics.declared_type_params();

        let bounds = fields.iter().filter_map(|f| {
            let ty = &f.ty;
            let usages = ty.uses_type_params_cloned(&search_options, &decl_tp);
            if usages.is_empty() {
                None
            } else {
                Some(quote! { #ty: ::orga::migrate::Migrate, })
            }
        });
        let wher = match wher {
            Some(wher) => quote! { #wher #(#bounds)*},
            None => quote! { where #(#bounds)* },
        };

        let prev_migration = if let Some(prev) = previous {
            quote! {
                let prev = <#prev as ::orga::migrate::Migrate>::migrate(src, dest, bytes)?;
                let value = <#prev as ::orga::migrate::MigrateInto::<Self>>::migrate_into(prev)?;
                Ok(value)
            }
        } else {
            quote! {
                Err(::orga::Error::App(format!(
                    "Unknown version {} for type {}",
                    bytes[0],
                    ::std::any::type_name::<Self>(),
                )))
            }
        };

        tokens.extend(quote! {
            impl #imp ::orga::migrate::Migrate for #ident #ty #wher
            {
                fn migrate(src: ::orga::store::Store, dest: ::orga::store::Store, mut bytes: &mut &[u8]) -> ::orga::Result<Self> {
                    if (::orga::compat_mode() && #version == 0)
                        || (!::orga::compat_mode() && bytes[0] == #version) {
                        if !::orga::compat_mode() {
                            *bytes = &bytes[1..];
                        }
                        return Ok(Self {
                            #(#field_migrations)*
                        });
                    }

                    #prev_migration
                }
            }
        })
    }
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    MigrateInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
