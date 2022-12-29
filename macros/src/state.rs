use super::utils::gen_param_input;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::str::FromStr;
use syn::*;

pub fn derive(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let mut is_tuple_struct = false;
    match &item.data {
        Data::Struct(data) => {
            if let Fields::Unnamed(_) = data.fields {
                is_tuple_struct = true
            }
        }
        _ => todo!("Currently only structs are supported"),
    }

    let field_names = || struct_fields(&item).map(|field| &field.ident);
    let seq =
        || (0..field_names().count()).map(|i| TokenStream2::from_str(&i.to_string()).unwrap());

    let name = &item.ident;
    let generics = &item.generics;
    let where_clause = &generics.where_clause;
    let generic_params = gen_param_input(&generics, true);

    let seq_substore = seq();
    let attach_body = if is_tuple_struct {
        let seq_field = seq();
        quote!(
            #(::orga::state::State::<::orga::store::DefaultBackingStore>::attach(&mut self.#seq_field, store.sub(&[#seq_substore]))?;)*
            Ok(())
        )
    } else {
        let names = field_names();
        quote!(
            #(::orga::state::State::<::orga::store::DefaultBackingStore>::attach(&mut self.#names, store.sub(&[#seq_substore]))?;)*
            Ok(())
        )
    };

    let flush_body = if is_tuple_struct {
        let indexes = seq();
        quote!(
            #(::orga::state::State::<::orga::store::DefaultBackingStore>::flush(&mut self.#indexes)?;)*
            Ok(())
        )
    } else {
        let names = field_names();
        quote!(
            #(::orga::state::State::<::orga::store::DefaultBackingStore>::flush(&mut self.#names)?;)*
            Ok(())
        )
    };

    let output = quote! {
        impl#generics ::orga::state::State for #name#generic_params
        #where_clause
        {
            fn attach(
                &mut self,
                store: ::orga::store::Store,
            ) -> ::orga::Result<()> {
                #attach_body
            }

            fn flush(
                &mut self,
            ) -> ::orga::Result<()> {
                #flush_body
            }
        }
    };

    output.into()
}

fn struct_fields(item: &DeriveInput) -> impl Iterator<Item = &Field> {
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
