use crate::utils::{is_attr_with_ident, path_to_ident};
use darling::{export::NestedMeta, FromMeta};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::{visit_mut::VisitMut, *};

type ChannelList = HashMap<Ident, ()>;

struct ChannelFilter {
    channel: Ident,
    all_channels: ChannelList,
}

impl VisitMut for ChannelFilter {
    fn visit_attribute_mut(&mut self, i: &mut Attribute) {
        if is_attr_with_ident(i, "channel") {
            let chan_attr = ChannelList::from_meta(&i.meta).unwrap();

            let target: Attribute = if chan_attr.contains_key(&self.channel) {
                // always true
                parse_quote! { #[cfg(all())]}
            } else {
                for chan in chan_attr.keys() {
                    if !self.all_channels.contains_key(chan) {
                        panic!("Unexpected channel: {}", chan)
                    }
                }
                // always false
                parse_quote! { #[cfg(any())]}
            };

            i.meta = target.meta;
        }

        visit_mut::visit_attribute_mut(self, i);
    }
}

fn add_channel_name(item: &mut Item, channel: &Ident) {
    match item {
        Item::Impl(item) => {
            if let box Type::Path(TypePath { path, .. }) = &mut item.self_ty {
                let generic_args = path.segments.last().unwrap().arguments.clone();
                let ident = path_to_ident(&path);
                let ident = format_ident!("{}{}", ident, channel);

                *path = parse_quote! {#ident #generic_args};
            }
        }
        Item::Enum(item) => {
            item.ident = format_ident!("{}{}", item.ident, channel);
        }
        Item::Struct(item) => {
            item.ident = format_ident!("{}{}", item.ident, channel);
        }
        _ => panic!("Unsupported item type"),
    }
}

pub fn channels(attr_args: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = NestedMeta::parse_meta_list(attr_args.into()).unwrap();
    let channels = ChannelList::from_list(&attr_args).unwrap();

    let item = parse_macro_input!(item as Item);
    let all_channels = channels.clone();

    let items = channels.into_keys().map(|channel| {
        let mut item = item.clone();
        add_channel_name(&mut item, &channel);
        visit_mut::visit_item_mut(
            &mut ChannelFilter {
                channel,
                all_channels: all_channels.clone(),
            },
            &mut item,
        );
        item
    });

    quote! { #(#items)* }.into()
}
