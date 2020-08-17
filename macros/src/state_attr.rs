use quote::quote;
use syn::*;
use super::*;

pub fn state(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream
) -> proc_macro::TokenStream {
    let mut item = parse_macro_input!(item as DeriveInput);

    let store_outer_param = item.generics.params
        .first()
        .expect("Expected a generic type parameter")
        .clone();
    let store_type_name = get_generic_param_name(&store_outer_param);

    let store_param: GenericArgument =
        parse_quote!(orga::store::Prefixed<orga::store::Shared<#store_type_name>>);
    for field in struct_fields_mut(&mut item) {
        add_store_param_to_field(field, &store_param);
    }

    let name = &item.ident;
    let field_names = struct_fields(&item)
        .map(|field| &field.ident);

    let output = quote! {
        #item

        impl<#store_outer_param> orga::state::State<#store_type_name> for #name<#store_type_name> {
            fn wrap_store(store: #store_type_name) -> orga::Result<Self> {
                use orga::store::Read;
                let mut splitter = orga::store::Splitter::new(store);
                Ok(Self {
                    #(
                        #field_names: splitter.split().wrap()?,
                    )*
                })
            }
        }
    };

    output.into()
}