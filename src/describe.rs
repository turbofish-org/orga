use crate::{
    encoding::{Decode, Encode},
    state::State,
    store::Store,
    Result,
};
use js_sys::Array;
use std::{
    any::Any,
    fmt::{Debug, Display},
};
use wasm_bindgen::prelude::*;

mod builder;

pub use builder::Builder;

pub trait Describe {
    fn describe() -> Descriptor;
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Descriptor {
    pub type_name: String,
    children: Children,
    decode: DecodeFn,
}

impl Debug for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Descriptor")
            .field("type_name", &self.type_name)
            .field("children", &self.children)
            .finish()
    }
}

#[wasm_bindgen]
impl Descriptor {
    #[wasm_bindgen(js_name = children)]
    pub fn children_js(&self) -> JsValue {
        use Children::*;

        match &self.children {
            None => JsValue::NULL,
            Named(children) => children
                .iter()
                .cloned()
                .map(JsValue::from)
                .collect::<Array>()
                .into(),
            Dynamic(child) => child.clone().into(),
        }
    }
}

pub type DecodeFn = fn(&[u8]) -> Result<Value>;

#[derive(Clone, Debug)]
pub enum Children {
    None,
    Named(Vec<NamedChild>),
    Dynamic(DynamicChild),
}

impl Default for Children {
    fn default() -> Self {
        Children::None
    }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct NamedChild {
    pub name: String,
    pub desc: Descriptor,
    store_key: KeyOp,
    access: AccessFn,
}

pub type AccessFn = fn(&Value) -> Result<Value>;

impl Debug for NamedChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamedChild")
            .field("name", &self.name)
            .field("desc", &self.desc)
            .field("store_key", &self.store_key)
            .finish()
    }
}

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct DynamicChild {
    key_desc: Box<Descriptor>,
    value_desc: Box<Descriptor>,
}

#[derive(Clone, Debug)]
pub enum KeyOp {
    Append(Vec<u8>),
    Absolute(Vec<u8>),
}

#[wasm_bindgen]
pub struct Value(Box<dyn Inspect>);

impl Value {
    pub fn new<T: Inspect + 'static>(instance: T) -> Self {
        Value(Box::new(instance))
    }

    pub fn to_any(&self) -> Result<Box<dyn Any>> {
        self.0.to_any()
    }
}

pub trait Inspect {
    fn maybe_display(&self) -> Option<String> {
        MaybeDisplay::maybe_display(&DisplayWrapper(&self))
    }

    fn maybe_debug(&self, alternate: bool) -> Option<String> {
        MaybeDebug::maybe_debug(&DebugWrapper(&self), alternate)
    }

    // TODO: should this be a maybe impl?
    fn encode(&self) -> Result<Vec<u8>>;

    // TODO: should this be a maybe impl?
    fn describe(&self) -> Descriptor;

    // TODO: should this be a maybe impl?
    fn attach(&mut self, store: Store) -> Result<()>;

    fn to_any(&self) -> Result<Box<dyn Any>>;

    // TODO: maybe_to_object
    // TODO: query
    // TODO: call
}

impl<T: State + Describe + 'static> Inspect for T {
    fn encode(&self) -> Result<Vec<u8>> {
        Ok(Encode::encode(self)?)
    }

    fn describe(&self) -> Descriptor {
        Self::describe()
    }

    fn attach(&mut self, store: Store) -> Result<()> {
        State::attach(self, store)
    }

    fn to_any(&self) -> Result<Box<dyn Any>> {
        let bytes = self.encode()?;
        let cloned = Self::decode(bytes.as_slice())?;
        Ok(Box::new(cloned))
    }
}

trait MaybeDisplay {
    fn maybe_display(&self) -> Option<String>;
}

struct DisplayWrapper<'a, T>(&'a T);

impl<'a, T> MaybeDisplay for DisplayWrapper<'a, T> {
    default fn maybe_display(&self) -> Option<String> {
        None
    }
}

impl<'a, T: Display> MaybeDisplay for DisplayWrapper<'a, T> {
    fn maybe_display(&self) -> Option<String> {
        Some(format!("{}", self.0))
    }
}

trait MaybeDebug {
    fn maybe_debug(&self, alternate: bool) -> Option<String>;
}

struct DebugWrapper<'a, T>(&'a T);

impl<'a, T> MaybeDebug for DebugWrapper<'a, T> {
    default fn maybe_debug(&self, _: bool) -> Option<String> {
        None
    }
}

impl<'a, T: Debug> MaybeDebug for DebugWrapper<'a, T> {
    fn maybe_debug(&self, alternate: bool) -> Option<String> {
        Some(if alternate {
            format!("{:#?}", self.0)
        } else {
            format!("{:?}", self.0)
        })
    }
}

macro_rules! primitive_impl {
    ($ty:ty) => {
        impl Describe for $ty {
            fn describe() -> Descriptor {
                Builder::new::<Self>().build()
            }
        }
    };
}

primitive_impl!(u8);
primitive_impl!(u16);
primitive_impl!(u32);
primitive_impl!(u64);
primitive_impl!(u128);
primitive_impl!(i8);
primitive_impl!(i16);
primitive_impl!(i32);
primitive_impl!(i64);
primitive_impl!(i128);
primitive_impl!(bool);
primitive_impl!(());
