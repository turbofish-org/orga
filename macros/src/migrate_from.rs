use darling::{ast, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::*;

#[derive(FromDeriveInput)]
#[darling(attributes(migrate_from), supports(struct_any))]
struct MigrateFromInputReceiver {
    ident: Ident,
    generics: Generics,
    data: ast::Data<(), MigrateFromFieldReceiver>,

    #[darling(default)]
    identity: bool,
}

impl ToTokens for MigrateFromInputReceiver {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let MigrateFromInputReceiver {
            ident,
            generics,
            data,
            identity,
        } = self;

        let (imp, ty, wher) = generics.split_for_impl();
        if *identity {
            return tokens.extend(quote! {
                impl #imp ::orga::migrate::MigrateFrom for #ident #ty
                #wher
                {
                    fn migrate_from(other: Self) -> ::orga::Result<Self> {
                        Ok(other)
                    }
                }
            });
        }

        let fields = data.as_ref().take_struct().unwrap().fields;

        let field_migrations = fields.iter().enumerate().map(|(i, f)| {
            let field_ident = f.ident.as_ref().map(|v| quote!(#v)).unwrap_or_else(|| {
                let i = syn::Index::from(i);
                quote!(#i)
            });

            quote! { #field_ident: ::orga::migrate::MigrateFrom::migrate_from(other.#field_ident)?,}
        });
        let bounds = fields.iter().map(|f| {
            let ty = &f.ty;
            quote! { #ty: ::orga::migrate::MigrateFrom }
        });
        let wher = match wher {
            Some(wher) => quote! { #wher #(#bounds,)* },
            None => quote! { where #(#bounds,)* },
        };

        tokens.extend(quote! {
            impl #imp ::orga::migrate::MigrateFrom for #ident #ty
            #wher
            {
                fn migrate_from(other: Self) -> ::orga::Result<Self> {
                    Ok(Self {
                        #(#field_migrations)*
                    })
                }
            }
        })
    }
}

#[derive(FromField)]
struct MigrateFromFieldReceiver {
    ident: Option<Ident>,
    ty: Type,
}

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    MigrateFromInputReceiver::from_derive_input(&item)
        .unwrap()
        .into_token_stream()
        .into()
}
