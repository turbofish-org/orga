use crate::{
    encoding::{Decode, Encode},
    Error, Result,
};
use std::{any::type_name, marker::PhantomData, str::FromStr};

use super::{
    AccessFn, Children, DecodeFn, Describe, Descriptor, DynamicChild, Inspect, KeyOp, NamedChild,
    ParseFn, Value,
};

pub struct Builder {
    type_name: String,
    decode: DecodeFn,
    parse: ParseFn,
    children: Option<Children>,
}

impl Builder {
    pub fn new<T: Encode + Decode + Inspect + 'static>() -> Self {
        Builder {
            type_name: type_name::<T>().to_string(),
            decode: |bytes| Ok(Value::new(T::decode(bytes)?)),
            parse: |s| maybe_from_str::<T>(s),
            children: None,
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

    pub fn dynamic_child<K: Describe, V: Describe>(mut self) -> Self {
        let child = DynamicChild {
            key_desc: Box::new(K::describe()),
            value_desc: Box::new(V::describe()),
        };

        match self.children {
            None => self.children = Some(Children::Dynamic(child)),
            Some(_) => panic!("Cannot add dynamic child"),
        };

        self
    }

    pub fn build(self) -> Descriptor {
        Descriptor {
            type_name: self.type_name,
            decode: Some(self.decode),
            parse: Some(self.parse),
            children: self.children.unwrap_or_default(),
        }
    }

    pub fn access<T: 'static, U: Encode + Decode + Inspect + 'static>(
        value: &Value,
        access: fn(T) -> U,
    ) -> Result<Option<Value>> {
        let cloned = value.to_any()?;
        let parent: T = *cloned.downcast().unwrap();
        let child = access(parent);
        Ok(Some(Value::new(child)))
    }

    pub fn maybe_access<T: 'static, U: Encode + Decode + Inspect + 'static>(
        value: &Value,
        access: fn(T) -> Option<U>,
    ) -> Result<Option<Value>> {
        let cloned = value.to_any()?;
        let parent: T = *cloned.downcast().unwrap();
        Ok(access(parent).map(|child| Value::new(child)))
    }
}

fn maybe_from_str<T>(s: &str) -> Result<Option<Value>> {
    FromStrWrapper::<T>::maybe_from_str(s)
}

trait MaybeFromStr {
    // TODO: Result
    fn maybe_from_str(s: &str) -> Result<Option<Value>>;
}

struct FromStrWrapper<T>(PhantomData<T>);

impl<T> MaybeFromStr for FromStrWrapper<T> {
    default fn maybe_from_str(_s: &str) -> Result<Option<Value>> {
        Ok(None)
    }
}

impl<T: FromStr + Inspect + 'static> MaybeFromStr for FromStrWrapper<T>
where
    Error: From<<T as FromStr>::Err>,
{
    fn maybe_from_str(s: &str) -> Result<Option<Value>> {
        Ok(Some(Value::new(T::from_str(s)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;

    #[derive(State, Encode, Decode)]
    struct Foo {
        bar: u32,
    }

    impl Describe for Foo {
        fn describe() -> Descriptor {
            Builder::new::<Self>()
                .named_child::<u32>("bar", &[0], |v| Builder::access(v, |v: Self| v.bar))
                .build()
        }
    }

    #[test]
    fn builder_named() {
        let desc = <Foo as Describe>::describe();
        assert_eq!(&desc.type_name, "orga::describe::builder::tests::Foo");
        match &desc.children {
            Children::Named(children) => {
                assert_eq!(children.len(), 1);
                assert_eq!(&children[0].name, "bar");
                assert_eq!(&children[0].store_key, &KeyOp::Append(vec![0]));
                assert_eq!(&children[0].desc.type_name, "u32");
            }
            _ => panic!("Incorrect children"),
        }
    }
}
