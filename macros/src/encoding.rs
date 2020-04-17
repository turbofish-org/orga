use super::*;

// TODO: use corrent spans show errors are shown on fields

pub fn derive_encode(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let name = &item.ident;
    
    let field_names: Vec<_> = struct_fields(&item)
        .map(|field| &field.ident)
        .collect();
    
    let mut field_names_minus_last: Vec<_> = struct_fields(&item)
        .map(|field| &field.ident)
        .collect();
    field_names_minus_last.pop();

    let output = quote! {
        impl orga::Encode for #name {
            fn encode_into<W: std::io::Write>(&self, mut dest: &mut W) -> orga::Result<()> {
                fn assert_trait_bounds<T: orga::Encode + orga::Terminated>(_: T) {}
                #(assert_trait_bounds(self.#field_names_minus_last);)*

                #(self.#field_names.encode_into(&mut dest)?;)*

                Ok(())
            }

            fn encoding_length(&self) -> orga::Result<usize> {
                Ok(
                    0 #( + self.#field_names.encoding_length()?)*
                )
            }
        }
    };
    
    output.into()
}

pub fn derive_decode(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let name = &item.ident;
    
    let field_names: Vec<_> = struct_fields(&item)
        .map(|field| &field.ident)
        .collect();

    let output = quote! {
        impl orga::Decode for #name {
            fn decode<R: std::io::Read>(mut input: R) -> orga::Result<Self> {
                Ok(Self {
                    #(
                        #field_names: orga::Decode::decode(&mut input)?,
                    )*
                })
            }

            fn decode_into<R: std::io::Read>(&mut self, mut input: R) -> orga::Result<()> {
                #(
                    self.#field_names.decode_into(&mut input)?;
                )*

                Ok(())
            }
        }
    };
    
    output.into()
}

pub fn derive_terminated(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(item as DeriveInput);

    let name = &item.ident;
    
    let field_names = struct_fields(&item)
        .map(|field| &field.ident);

    let assert_fn_name = Ident::new(
        format!("__assert_fields_are_terminated_{}", name).as_str(),
        proc_macro2::Span::call_site()
    );

    let output = quote! {
        impl orga::Terminated for #name {}

        #[allow(unused_variables)]
        fn #assert_fn_name(s: #name) {
            fn assert_terminated<T: Terminated>(_: T) {}
            #(assert_terminated(s.#field_names);)*
        }
    };
    
    output.into()
}
