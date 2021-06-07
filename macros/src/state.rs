use std::str::FromStr;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let field_names = || struct_fields(&item).map(|field| &field.ident);
    let field_types = || struct_fields(&item).map(|field| &field.ty);
    let seq = || {
        (0..field_names().count())
            .map(|i| TokenStream2::from_str(&i.to_string()).unwrap())
    };

    let name = &item.ident;
    let field_types_encoding = field_types();
    let field_names_create = field_names();
    let seq_substore = seq();
    let seq_data = seq();
    let field_names_flush = field_names();

    let output = quote! {
        impl ::orga::state::State for #name {
            type Encoding = (
                #(
                    <#field_types_encoding as ::orga::state::State>
                        ::Encoding,
                )* 
            );

            fn create(
                store: ::orga::store::Store,
                data: Self::Encoding,
            ) -> ::orga::Result<Self> {
                Ok(Self {
                    #(
                        #field_names_create: ::orga::state::State::create(
                            store.sub(&[#seq_substore]),
                            data.#seq_data,
                        )?,
                    )*
                })
            }

            fn flush(self) -> ::orga::Result<Self::Encoding> {
                Ok((
                    #(self.#field_names_flush.flush()?,)*
                ))
            }
        }
    };

    output.into()
}

fn struct_fields<'a>(
    item: &'a DeriveInput
) -> impl Iterator<Item=&'a Field> {
    let data = match item.data {
        Data::Struct(ref data) => data,
        Data::Enum(ref _data) => todo!("#[derive(State)] does not yet support enums"),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    match data.fields {
        Fields::Named(ref fields) => fields.named.iter(),
        Fields::Unnamed(ref fields) => fields.unnamed.iter(),
        Fields::Unit => panic!("Unit structs are not supported"),
    }
}
