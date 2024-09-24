use darling::util::path_to_string;
use heck::{CamelCase, SnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use regex::Regex;
use std::collections::HashSet;
use syn::Ident;
use syn::*;

pub fn _parse_parent() -> File {
    let path = proc_macro::Span::call_site().source_file().path();
    let source = std::fs::read_to_string(path).unwrap_or_default();
    parse_file(source.as_str()).unwrap()
}

pub fn _get_generic_requirements<I, J>(inputs: I, params: J) -> Vec<Ident>
where
    I: Iterator<Item = Type>,
    J: Iterator<Item = GenericParam>,
{
    let params = params.collect::<Vec<_>>();
    let maybe_generic_inputs = inputs
        .filter_map(|input| match input {
            Type::Path(path) => Some(path),
            Type::Reference(reference) => match *reference.elem {
                Type::Path(path) => Some(path),
                _ => None,
            },
            _ => None,
        })
        .flat_map(|path| {
            let mut paths = vec![];
            fn add_arguments(path: &TypePath, paths: &mut Vec<PathSegment>) {
                let last = path.path.segments.last().unwrap();
                paths.push(last.clone());
                if let PathArguments::AngleBracketed(ref args) = last.arguments {
                    for arg in args.args.iter() {
                        if let GenericArgument::Type(ty) = arg {
                            let maybe_path = match ty {
                                Type::Path(path) => Some(path),
                                Type::Reference(reference) => match *reference.elem {
                                    Type::Path(ref path) => Some(path),
                                    _ => None,
                                },
                                _ => None,
                            };
                            maybe_path.map(|path| add_arguments(path, paths));
                        }
                    }
                }
            }
            add_arguments(&path, &mut paths);

            paths
        });
    let mut requirements = vec![];
    for input in maybe_generic_inputs {
        params
            .iter()
            .filter_map(|param| match param {
                GenericParam::Type(param) => Some(param),
                _ => None,
            })
            .find(|param| param.ident == input.ident)
            .map(|param| {
                requirements.push(param.ident.clone());
            });
    }
    let req_set: HashSet<_> = requirements.into_iter().collect();
    req_set.into_iter().collect()
}

pub fn _relevant_impls(names: Vec<&Ident>, source: &File) -> Vec<ItemImpl> {
    source
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(item) => Some(item),
            _ => None,
        })
        .filter(|item| item.trait_.is_none())
        .filter(|item| {
            let path = match &*item.self_ty {
                Type::Path(path) => path,
                _ => return false,
            };

            if path.qself.is_some() {
                return false;
            }
            if path.path.segments.len() != 1 {
                return false;
            }
            if !names.contains(&&path.path.segments[0].ident) {
                return false;
            }

            true
        })
        .cloned()
        .collect()
}

pub fn _relevant_methods(name: &Ident, attr: &str, source: &File) -> Vec<(ImplItemFn, ItemImpl)> {
    let get_methods = |item: ItemImpl| -> Vec<_> {
        item.items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .filter(|method| {
                method
                    .attrs
                    .iter()
                    .find(|a| a.path().is_ident(&attr))
                    .is_some()
            })
            .filter(|method| matches!(method.vis, Visibility::Public(_)))
            .filter(|method| method.sig.unsafety.is_none())
            .filter(|method| method.sig.asyncness.is_none())
            .filter(|method| method.sig.abi.is_none())
            .map(|method| (method.clone(), item.clone()))
            .collect()
    };

    _relevant_impls(vec![name, &_strip_version(name)], source)
        .into_iter()
        .flat_map(get_methods)
        .collect()
}

pub fn gen_param_input(generics: &Generics, bracketed: bool) -> TokenStream {
    let gen_params = generics.params.iter().map(|p| match p {
        GenericParam::Type(p) => {
            let ident = &p.ident;
            quote!(#ident)
        }
        GenericParam::Lifetime(p) => {
            let ident = &p.lifetime.ident;
            quote!(#ident)
        }
        GenericParam::Const(p) => {
            let ident = &p.ident;
            quote!(#ident)
        }
    });

    if gen_params.len() == 0 {
        quote!()
    } else if bracketed {
        quote!(<#(#gen_params),*>)
    } else {
        quote!(#(#gen_params),*)
    }
}

pub fn _strip_version(ident: &Ident) -> Ident {
    let name = ident.to_string();
    let re = Regex::new(r"V([0-9]+)").unwrap();
    let stripped_name = re.replace_all(&name, "").to_string();
    Ident::new(stripped_name.as_str(), ident.span())
}

macro_rules! named_fields {
    ($target:ident) => {
        $target
            .data
            .as_ref()
            .take_struct()
            .unwrap()
            .fields
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                (f.ident.as_ref().map(|v| quote!(#v)).unwrap_or_else(|| {
                    let i = syn::Index::from(i);
                    quote!(#i)
                }), f)
            })
    };
}

pub(crate) use named_fields;

#[allow(dead_code)]
pub struct Types {
    pub state_trait: TokenStream,
    pub store_ty: TokenStream,
    pub attacher_ty: TokenStream,
    pub flusher_ty: TokenStream,
    pub loader_ty: TokenStream,
    pub result_ty: TokenStream,
    pub error_ty: TokenStream,
    pub terminated_trait: TokenStream,
    pub encode_trait: TokenStream,
    pub decode_trait: TokenStream,
    pub encoder_ty: TokenStream,
    pub decoder_ty: TokenStream,
    pub field_call_trait: TokenStream,
    pub method_call_trait: TokenStream,
    pub call_trait: TokenStream,
    pub trace_fn: TokenStream,
    pub maybe_pop_trace_fn: TokenStream,
    pub maybe_push_trace_fn: TokenStream,
    pub trace_method_type_enum: TokenStream,
    pub keyop_ty: TokenStream,
    pub ed_result_ty: TokenStream,
    pub ed_error_ty: TokenStream,
    pub call_item_ty: TokenStream,
    pub child_trait: TokenStream,
    pub child_field_ty: TokenStream,
    pub build_call_trait: TokenStream,
    pub call_builder_ty: TokenStream,
    pub query_trait: TokenStream,
    pub field_query_trait: TokenStream,
    pub method_query_trait: TokenStream,
    pub query_item_ty: TokenStream,
}

impl Default for Types {
    fn default() -> Self {
        Self {
            state_trait: quote! { ::orga::state::State },
            store_ty: quote! { ::orga::store::Store },
            attacher_ty: quote! { ::orga::state::Attacher },
            flusher_ty: quote! { ::orga::state::Flusher },
            loader_ty: quote! { ::orga::state::Loader },
            result_ty: quote! { ::orga::Result },
            error_ty: quote! { ::orga::Error },
            terminated_trait: quote! { ::orga::encoding::Terminated },
            encode_trait: quote! { ::orga::encoding::Encode },
            decode_trait: quote! { ::orga::encoding::Decode },
            encoder_ty: quote! { ::orga::encoding::encoder::Encoder },
            decoder_ty: quote! { ::orga::encoding::decoder::Decoder },
            field_call_trait: quote! { ::orga::call::FieldCall },
            method_call_trait: quote! { ::orga::call::MethodCall },
            call_trait: quote! { ::orga::call::Call },
            trace_fn: quote! { ::orga::client::trace::push_trace },
            maybe_pop_trace_fn: quote! { ::orga::client::trace::maybe_pop_trace },
            maybe_push_trace_fn: quote! { ::orga::client::trace::maybe_push_trace },
            trace_method_type_enum: quote! { ::orga::client::trace::MethodType },
            keyop_ty: quote! { ::orga::describe::KeyOp },
            ed_result_ty: quote! { ::orga::encoding::Result },
            ed_error_ty: quote! { ::orga::encoding::Error },
            call_item_ty: quote! { ::orga::call::Item },
            child_trait: quote! { ::orga::describe::child::Child },
            child_field_ty: quote! { ::orga::describe::child::Field },
            build_call_trait: quote! { ::orga::call::BuildCall },
            call_builder_ty: quote! { ::orga::call::CallBuilder },
            query_trait: quote! { ::orga::query::Query },
            field_query_trait: quote! { ::orga::query::FieldQuery },
            method_query_trait: quote! { ::orga::query::MethodQuery },
            query_item_ty: quote! { ::orga::query::Item },
        }
    }
}

pub fn is_attr_with_ident(attr: &Attribute, ident: &str) -> bool {
    attr.path()
        .get_ident()
        .map_or(false, |attr_ident| attr_ident.to_string() == ident)
}

pub fn to_camel_case(ident: &Ident) -> Ident {
    Ident::new(&format!("{}", ident).as_str().to_camel_case(), ident.span())
}

pub fn to_snake_case(ident: &Ident) -> Ident {
    Ident::new(&format!("{}", ident).as_str().to_snake_case(), ident.span())
}

pub fn _impl_item_attrs(item: &ImplItem) -> Vec<Attribute> {
    use ImplItem::*;
    match item {
        Fn(method) => method.attrs.clone(),
        Const(constant) => constant.attrs.clone(),
        Type(ty) => ty.attrs.clone(),
        Macro(macro_) => macro_.attrs.clone(),
        _ => unimplemented!(),
    }
}

pub fn path_to_ident(path: &Path) -> Ident {
    format_ident!("{}", path_to_string(path))
}

pub fn _generics_union(gen_a: &Generics, gen_b: &Generics) -> Generics {
    let mut generics = gen_a.clone();
    let params = gen_b.params.clone();
    for param in params.iter() {
        generics.params.push(param.clone());
    }
    if let Some(wc) = gen_b.where_clause.as_ref() {
        let wher = generics.make_where_clause();

        for pred in wc.predicates.iter() {
            wher.predicates.push(pred.clone());
        }
    }
    generics
}

pub fn _replace_type_segment(ty: &mut Type, target: &Ident, replacement: &Ident) {
    use syn::{visit_mut::VisitMut, *};

    struct TypeReplacer {
        target: Ident,
        replacement: Ident,
    }

    impl VisitMut for TypeReplacer {
        fn visit_path_segment_mut(&mut self, segment: &mut PathSegment) {
            if segment.ident == self.target {
                segment.ident = self.replacement.clone()
            }

            self.visit_path_arguments_mut(&mut segment.arguments);
        }
    }

    let mut replacer = TypeReplacer {
        target: target.clone(),
        replacement: replacement.clone(),
    };
    replacer.visit_type_mut(ty);
}
