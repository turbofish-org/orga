use crate::{
    encoding::{Decode, Encode},
    Result,
};
use std::any::type_name;

use super::{
    AccessFn, Children, DecodeFn, Describe, Descriptor, DynamicChild, Inspect, KeyOp, NamedChild,
    Value,
};

pub struct Builder {
    type_name: String,
    decode: DecodeFn,
    children: Option<Children>,
}

impl Builder {
    pub fn new<T: Encode + Decode + Inspect + 'static>() -> Self {
        Builder {
            type_name: type_name::<T>().to_string(),
            decode: |bytes| Ok(Value::new(T::decode(bytes)?)),
            children: None,
        }
    }

    pub fn named_child<T: Describe>(
        mut self,
        name: &'static str,
        store_suffix: &[u8],
        access: AccessFn,
    ) -> Self {
        let child = NamedChild {
            name: name.to_string(),
            store_key: KeyOp::Append(store_suffix.to_vec()),
            desc: T::describe(),
            access,
        };

        match self.children {
            None => self.children = Some(Children::Named(vec![child])),
            Some(Children::Named(ref mut children)) => children.push(child),
            Some(_) => panic!("Cannot add named child"),
        };

        self
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
            decode: self.decode,
            children: self.children.unwrap_or_default(),
        }
    }

    pub fn access<T: 'static, U: Encode + Decode + Inspect + 'static>(
        value: &Value,
        access: fn(T) -> U,
    ) -> Result<Value> {
        let cloned = value.to_any()?;
        let parent: T = *cloned.downcast().unwrap();
        let child = access(parent);
        Ok(Value::new(child))
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
    fn builder() {
        dbg!(<Foo as Describe>::describe());
    }
}
