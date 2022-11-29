use crate::{
    encoding::{Decode, Encode},
    Result,
};
use std::{
    any::Any,
    fmt::{Debug, Display},
};

mod builder;

pub use builder::Builder;

pub trait Describe {
    fn describe() -> Descriptor;
}

#[derive(Clone)]
pub struct Descriptor {
    pub type_name: String,
    pub children: Children,
    decode: DecodeFn,
}

pub type DecodeFn = fn(&[u8]) -> Result<Value>;

#[derive(Clone)]
pub enum Children {
    Named(Vec<NamedChild>),
    Dynamic(DynamicChild),
}

impl Default for Children {
    fn default() -> Self {
        Children::Named(vec![])
    }
}

#[derive(Clone)]
pub struct NamedChild {
    pub name: String,
    pub store_key: KeyOp,
    pub desc: Descriptor,
    access: AccessFn,
}

pub type AccessFn = fn(&Value) -> Result<Value>;

#[derive(Clone)]
pub struct DynamicChild {
    pub key_desc: Box<Descriptor>,
    pub value_desc: Box<Descriptor>,
}

#[derive(Clone, Debug)]
pub enum KeyOp {
    Append(Vec<u8>),
    Absolute(Vec<u8>),
}

pub struct Value(Box<dyn Inspect>);

impl Value {
    pub fn new<T: Inspect + 'static>(instance: T) -> Self {
        Value(Box::new(instance))
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

    fn to_any(&self) -> Result<Box<dyn Any>>;

    // TODO: maybe_to_object
    // TODO: query
    // TODO: call
}

impl<T: Encode + Decode + Describe + 'static> Inspect for T {
    fn encode(&self) -> Result<Vec<u8>> {
        Ok(self.encode()?)
    }

    fn describe(&self) -> Descriptor {
        Self::describe()
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
