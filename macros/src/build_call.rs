use crate::utils::Types;
use proc_macro::TokenStream;

use quote::{format_ident, quote};
use syn::*;

pub fn build_call(item: TokenStream) -> TokenStream {
    let Types {
        call_builder_ty, ..
    } = Types::default();
    let item = parse_macro_input!(item as ExprMethodCall);
    fn get_members(field: Expr, mut members: Vec<Ident>) -> Vec<Ident> {
        match field {
            Expr::Field(ExprField { base, member, .. }) => {
                let mut path = get_members(*base, members);
                let ident = match member {
                    Member::Named(ident) => ident,
                    Member::Unnamed(index) => format_ident!("{}", index.index),
                };
                path.push(ident);
                path
            }
            Expr::Path(ExprPath { path, .. }) => {
                members.push(path.get_ident().unwrap().clone());
                members
            }
            _ => panic!("unexpected member {:?}", field),
        }
    }
    let members = get_members(*(item.receiver.clone()), vec![]);
    let method_name = item.method.to_string();
    let method_args = &item.args;
    let method_args = match item.args.len() {
        0 => quote! { () },
        _ => quote! { (#method_args,) },
    };
    let output = members.iter().enumerate().rev().fold(
        quote! {
            .build_call::<#method_name, _>(|_|unreachable!(), #method_args)
        },
        |acc, (i, member)| {
            if i == 0 {
                quote! {
                    #call_builder_ty::make(#member) #acc
                }
            } else {
                let member = member.to_string();
                quote! {
                    .build_call::<#member, _>(|builder| builder #acc, ())
                }
            }
        },
    );

    output.into()
}
