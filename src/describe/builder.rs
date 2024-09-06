use crate::state::State;
use std::any::{type_name, TypeId};

use super::{
    ApplyQueryBytesFn, Children, Describe, Descriptor, DynamicChild, Inspect, KeyOp, LoadFn,
    NamedChild,
};

/// A builder for creating a [Descriptor].
pub struct Builder {
    type_id: TypeId,
    type_name: String,
    state_version: u32,
    load: LoadFn,
    children: Option<Children>,
    meta: Option<Box<Descriptor>>,
}

impl Builder {
    /// Create a new builder for a given type.
    pub fn new<T: State + Inspect + 'static>() -> Self {
        Builder {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>().to_string(),
            state_version: 0, // TODO
            load: |store, bytes| {
                T::load(store, bytes)?;
                Ok(())
            },
            // meta: Some(Box::new(<u8 as Describe>::describe())),
            meta: None,
            children: None,
        }
    }

    /// Add a [NamedChild] to the descriptor with a given key operation.
    pub fn named_child_keyop<T: Describe>(mut self, name: &'static str, keyop: KeyOp) -> Self {
        let child = NamedChild {
            name: name.to_string(),
            store_key: keyop,
            desc: T::describe(),
        };

        match self.children {
            None => self.children = Some(Children::Named(vec![child])),
            Some(Children::Named(ref mut children)) => children.push(child),
            Some(_) => panic!("Cannot add named child"),
        };

        self
    }

    /// Add a [NamedChild] to the descriptor with the given store key suffix
    /// appended.
    pub fn named_child<T: Describe>(self, name: &'static str, store_suffix: &[u8]) -> Self {
        self.named_child_keyop::<T>(name, KeyOp::Append(store_suffix.to_vec()))
    }

    /// Add a [NamedChild] to the descriptor using the [KeyOp] defined by the
    /// type's implementation of [State::field_keyop] for that field.
    pub fn named_child_from_state<T: State + Describe, U: Describe>(
        self,
        name: &'static str,
    ) -> Self {
        if let Some(keyop) = T::field_keyop(name) {
            self.named_child_keyop::<U>(name, keyop)
        } else {
            self
        }
    }

    /// Sets the [Meta] for the descriptor to the descriptor of another type
    /// `T`.
    pub fn meta<T: Describe>(self) -> Self {
        Builder {
            meta: Some(Box::new(T::describe())),
            ..self
        }
    }

    /// Sets the descriptor's children to [DynamicChild] with the provided
    /// [ApplyQueryBytesFn].
    pub fn dynamic_child<K: Describe, V: Describe>(
        mut self,
        apply_query_bytes: ApplyQueryBytesFn,
    ) -> Self {
        let child = DynamicChild {
            key_desc: Box::new(K::describe()),
            value_desc: Box::new(V::describe()),
            apply_query_bytes,
        };

        match self.children {
            None => self.children = Some(Children::Dynamic(child)),
            Some(_) => panic!("Cannot add dynamic child"),
        };

        self
    }

    /// Builds the descriptor.
    pub fn build(self) -> Descriptor {
        Descriptor {
            type_id: self.type_id,
            type_name: self.type_name,
            state_version: self.state_version,
            load: Some(self.load),
            children: self.children.unwrap_or_default(),
            meta: self.meta,
        }
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
//                 .named_child::<u32>("bar", &[0], |v| Builder::access(v, |v:
// Self| v.bar))                 .build()
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
