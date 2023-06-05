use crate::encoding::{Decode, Encode};
use crate::query::{MethodQuery, QueryMethodDescriptor};
use crate::state::State;
use crate::{Error, Result};
use std::any::{type_name, TypeId};
use std::marker::PhantomData;
use std::str::FromStr;

use super::{
    value::Value, AccessFn, ApplyQueryBytesFn, Children, Describe, Descriptor, DynamicChild,
    Inspect, KeyOp, LoadFn, NamedChild,
};
use super::{DynamicAccessRefFn, InspectRef, ParseFn};

pub struct Builder {
    type_id: TypeId,
    type_name: String,
    state_version: u32,
    query_methods: Vec<QueryMethodDescriptor>,
    load: LoadFn,
    children: Option<Children>,
    meta: Option<Box<Descriptor>>,
    parse: ParseFn,
}

impl Builder {
    pub fn new<T: State + Inspect + 'static>() -> Self {
        Builder {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>().to_string(),
            state_version: 0, // TODO
            query_methods: vec![],
            load: |store, bytes| Ok(Box::new(T::load(store, bytes)?)),
            meta: None,
            children: None,
            parse: |s| maybe_from_str::<T>(s),
        }
    }

    pub fn named_child_keyop<T: Describe>(
        mut self,
        name: &'static str,
        keyop: KeyOp,
        access: AccessFn,
    ) -> Self {
        let child = NamedChild {
            name: name.to_string(),
            store_key: keyop,
            desc: T::describe(),
            access: Some(access),
        };

        match self.children {
            None => self.children = Some(Children::Named(vec![child])),
            Some(Children::Named(ref mut children)) => children.push(child),
            Some(_) => panic!("Cannot add named child"),
        };

        self
    }

    pub fn named_child<T: Describe>(
        self,
        name: &'static str,
        store_suffix: &[u8],
        access: AccessFn,
    ) -> Self {
        self.named_child_keyop::<T>(name, KeyOp::Append(store_suffix.to_vec()), access)
    }

    pub fn named_child_from_state<T: State + Describe, U: Describe>(
        self,
        name: &'static str,
        access: AccessFn,
    ) -> Self {
        if let Some(keyop) = T::field_keyop(name) {
            return self.named_child_keyop::<U>(name, keyop, access);
        } else {
            self
        }
    }

    pub fn meta<T: Describe>(self) -> Self {
        Builder {
            meta: Some(Box::new(T::describe())),
            ..self
        }
    }

    pub fn dynamic_child<K: Describe, V: Describe>(
        mut self,
        apply_query_bytes: ApplyQueryBytesFn,
        access_ref: DynamicAccessRefFn,
    ) -> Self {
        let child = DynamicChild {
            key_desc: Box::new(K::describe()),
            value_desc: Box::new(V::describe()),
            apply_query_bytes,
            access_ref,
        };

        match self.children {
            None => self.children = Some(Children::Dynamic(child)),
            Some(_) => panic!("Cannot add dynamic child"),
        };

        self
    }

    pub fn query_methods<T: MethodQuery>(mut self) -> Self {
        self.query_methods = T::describe_methods();
        self
    }

    pub fn access<T: Inspect + 'static, U: Inspect + 'static>(
        value: InspectRef,
        access: fn(&T) -> &U,
    ) -> Result<InspectRef> {
        use std::any::Any;
        let any: &dyn Any = value as _;
        let parent: &T = any.downcast_ref().unwrap();
        let child = access(parent);

        Ok(child)
    }

    pub fn build(self) -> Descriptor {
        Descriptor {
            type_id: self.type_id,
            type_name: self.type_name,
            state_version: self.state_version,
            query_methods: self.query_methods,
            load: Some(self.load),
            children: self.children.unwrap_or_default(),
            meta: self.meta,
            parse: Some(self.parse),
        }
    }
}

fn maybe_from_str<T>(s: &str) -> Result<Option<Box<dyn Inspect>>> {
    FromStrWrapper::<T>::maybe_from_str(s)
}

trait MaybeFromStr {
    fn maybe_from_str(s: &str) -> Result<Option<Box<dyn Inspect>>>;
}

struct FromStrWrapper<T>(PhantomData<T>);

impl<T> MaybeFromStr for FromStrWrapper<T> {
    default fn maybe_from_str(_s: &str) -> Result<Option<Box<dyn Inspect>>> {
        Ok(None)
    }
}

impl<T: FromStr + Inspect + 'static> MaybeFromStr for FromStrWrapper<T>
where
    Error: From<<T as FromStr>::Err>,
{
    fn maybe_from_str(s: &str) -> Result<Option<Box<dyn Inspect>>> {
        Ok(Some(Box::new(T::from_str(s)?)))
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::state::State;

//     #[derive(State, Encode, Decode)]
//     struct Foo {
//         bar: u32,
//     }

//     impl Describe for Foo {
//         fn describe() -> Descriptor {
//             Builder::new::<Self>()
//                 .named_child::<u32>("bar", &[0], |v| Builder::access(v, |v: Self| v.bar))
//                 .build()
//         }
//     }

//     #[test]
//     fn builder_named() {
//         let desc = <Foo as Describe>::describe();
//         assert_eq!(&desc.type_name, "orga::describe::builder::tests::Foo");
//         match &desc.children {
//             Children::Named(children) => {
//                 assert_eq!(children.len(), 1);
//                 assert_eq!(&children[0].name, "bar");
//                 assert_eq!(&children[0].store_key, &KeyOp::Append(vec![0]));
//                 assert_eq!(&children[0].desc.type_name, "u32");
//             }
//             _ => panic!("Incorrect children"),
//         }
//     }
// }
