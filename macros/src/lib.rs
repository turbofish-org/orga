use proc_macro2::{TokenStream, Span};
use quote::{quote, quote_spanned, ToTokens};
use syn::{*, punctuated::*};

#[proc_macro_attribute]
pub fn state(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream
) -> proc_macro::TokenStream {
    let mut item = parse_macro_input!(item as DeriveInput);

    let store_param: GenericParam = parse_quote!(S: orga::Store);
    item.generics.params.insert(0, store_param);

    struct_fields_mut(&mut item)
        .for_each(add_store_param_to_field);

    let name = &item.ident;
    let field_names = struct_fields(&item)
        .map(|field| &field.ident);

    (quote! {
        #item

        impl<S: orga::Store> WrapStore<S> for #name<S> {
            fn wrap_store(store: S) -> orga::Result<Self> {
                let mut splitter = orga::Splitter::new(store);
                Ok(Self {
                    #(
                        #field_names: orga::WrapStore::wrap_store(splitter.split())?,
                    )*
                })
            }
        }
    }).into()
}

fn struct_fields<'a>(
    item: &'a DeriveInput
) -> impl Iterator<Item=&'a Field> {
    let data = match item.data {
        Data::Struct(ref data) => data,
        _ => panic!("The #[state] attribute can only be used on structs")
    };
    match data.fields {
        Fields::Named(ref fields) => fields.named.iter(),
        Fields::Unnamed(ref fields) => fields.unnamed.iter(),
        Fields::Unit => panic!("Unit structs are not supported")
    }
}

fn struct_fields_mut<'a>(
    item: &'a mut DeriveInput
) -> impl Iterator<Item=&'a mut Field> {
    let mut data = match item.data {
        Data::Struct(ref mut data) => data,
        _ => panic!("The #[state] attribute can only be used on structs")
    };
    match data.fields {
        Fields::Named(ref mut fields) => fields.named.iter_mut(),
        Fields::Unnamed(ref mut fields) => fields.unnamed.iter_mut(),
        Fields::Unit => panic!("Unit structs are not supported")
    }
}

fn add_store_param_to_field(field: &mut Field) {
    let ty = match field.ty {
        Type::Path(ref mut ty) => ty,
        _ => unimplemented!("must have path type")
    };

    let store_param: GenericArgument =
        parse_quote!(orga::split::Substore<S>);
    let base = ty.path.segments.last_mut().unwrap();

    match &mut base.arguments {
        PathArguments::AngleBracketed(args) => {
            args.args.insert(0, store_param);
        },
        PathArguments::None => {
            let args = parse_quote!(<#store_param>);
            base.arguments = PathArguments::AngleBracketed(args);
        },
        PathArguments::Parenthesized(_) => {
            panic!("Unexpected parenthesized type arguments")
        }
    };
}
