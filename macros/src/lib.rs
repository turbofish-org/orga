use quote::quote;
use syn::*;

mod encoding;
mod state_attr;

#[proc_macro_attribute]
pub fn state(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream
) -> proc_macro::TokenStream {
    state_attr::state(attr, item)
}

#[proc_macro_derive(Encode)]
pub fn encode(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    encoding::derive_encode(item)
}

#[proc_macro_derive(Decode)]
pub fn decode(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    encoding::derive_decode(item)
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
    let data = match item.data {
        Data::Struct(ref mut data) => data,
        _ => panic!("The #[state] attribute can only be used on structs")
    };
    match data.fields {
        Fields::Named(ref mut fields) => fields.named.iter_mut(),
        Fields::Unnamed(ref mut fields) => fields.unnamed.iter_mut(),
        Fields::Unit => panic!("Unit structs are not supported")
    }
}

fn add_store_param_to_field(
    field: &mut Field,
    param: &GenericArgument
) {
    let ty = match field.ty {
        Type::Path(ref mut ty) => ty,
        _ => unimplemented!("must have path type")
    };

    let base = ty.path.segments.last_mut().unwrap();

    match &mut base.arguments {
        PathArguments::AngleBracketed(args) => {
            args.args.insert(0, param.clone());
        },
        PathArguments::None => {
            let args = parse_quote!(<#param>);
            base.arguments = PathArguments::AngleBracketed(args);
        },
        PathArguments::Parenthesized(_) => {
            panic!("Unexpected parenthesized type arguments")
        }
    };
}

fn get_generic_param_name<'a>(param: &'a GenericParam) -> &'a Ident {
    match param {
        GenericParam::Type(type_param) => &type_param.ident,
        _ => panic!("must be type argument")
    }
}
