use crate::{
    encoding::{Decode, Encode},
    state::State,
    store::Store,
    Error, Result,
};
use ed::Terminated;
use serde::{Deserialize, Serialize};
use std::{
    any::{Any, TypeId},
    fmt::{Debug, Display},
};
use wasm_bindgen::prelude::*;

mod builder;
pub mod child;

pub use crate::macros::Describe;
pub use builder::Builder;

pub trait Describe {
    fn describe() -> Descriptor;
}

// #[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct Descriptor {
    pub type_id: TypeId,
    pub type_name: String,
    pub state_version: u32,
    children: Children,
    pub load: Option<LoadFn>,
    pub meta: Option<Box<Self>>,
}

impl Debug for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Descriptor")
            .field("type_name", &self.type_name)
            .field("state_version", &self.state_version)
            .field("children", &self.children)
            .finish()
    }
}

impl Descriptor {
    pub fn children(&self) -> &Children {
        &self.children
    }

    // pub fn kv_descs(self) -> impl Iterator<Item = DynamicChild> {
    //     let (own, named) = match self.children {
    //         Children::None => (vec![], vec![]),
    //         Children::Named(children) => (vec![], children.into_iter().map(|c| c.desc).collect()),
    //         Children::Dynamic(child) => (vec![child], vec![]),
    //     };
    //     own.into_iter()
    //         .chain(named.into_iter().map(Descriptor::kv_descs).flatten())
    // }
}

pub type LoadFn = fn(Store, &mut &[u8]) -> Result<()>;
pub type ApplyQueryBytesFn = fn(Vec<u8>) -> Vec<u8>;

#[derive(Clone, Debug, Default)]
pub enum Children {
    #[default]
    None,
    Named(Vec<NamedChild>),
    Dynamic(DynamicChild),
}

// #[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct NamedChild {
    pub name: String,
    pub desc: Descriptor,
    pub store_key: KeyOp,
}

// #[wasm_bindgen(inspectable)]
#[derive(Clone, Debug)]
pub struct DynamicChild {
    key_desc: Box<Descriptor>,
    value_desc: Box<Descriptor>,
    apply_query_bytes: ApplyQueryBytesFn,
}

impl DynamicChild {
    pub fn key_desc(&self) -> &Descriptor {
        &self.key_desc
    }

    pub fn value_desc(&self) -> &Descriptor {
        &self.value_desc
    }

    pub fn apply_query_bytes(&self, bytes: Vec<u8>) -> Vec<u8> {
        (self.apply_query_bytes)(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum KeyOp {
    Append(Vec<u8>),
    Absolute(Vec<u8>),
}

impl KeyOp {
    pub fn apply(&self, store: &Store) -> Store {
        match self {
            KeyOp::Absolute(prefix) => store.with_prefix(prefix.clone()),
            KeyOp::Append(prefix) => store.sub(prefix.as_slice()),
        }
    }

    pub fn apply_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        match self {
            KeyOp::Absolute(prefix) => prefix.clone(),
            KeyOp::Append(prefix) => {
                let mut bytes = bytes.to_vec();
                bytes.extend_from_slice(prefix.as_slice());
                bytes
            }
        }
    }
}

pub trait Inspect {
    fn maybe_to_string(&self) -> Option<String> {
        MaybeDisplay::maybe_to_string(&DisplayWrapper(&self))
    }

    fn maybe_debug(&self, alternate: bool) -> Option<String> {
        MaybeDebug::maybe_debug(&DebugWrapper(&self), alternate)
    }

    fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        MaybeToJson::maybe_to_json(&ToJsonWrapper(&self))
    }

    fn maybe_write_json(&self, out: Box<dyn std::io::Write>) -> Result<()> {
        MaybeToJson::maybe_write_json(&ToJsonWrapper(&self), out)
    }

    fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>> {
        MaybeToJson::maybe_to_wasm(&ToJsonWrapper(&self))
    }

    // TODO: should this be a maybe impl?
    fn encode(&self) -> Result<Vec<u8>> {
        unimplemented!()
    }

    // TODO: should this be a maybe impl?
    fn describe(&self) -> Descriptor;

    // TODO: should this be a maybe impl?
    fn attach(&mut self, store: Store) -> Result<()>;

    // TODO: should this be a maybe impl?
    fn state_version(&self) -> u32;

    fn to_any(&self) -> Result<Box<dyn Any>>;

    // TODO: maybe_to_object
    // TODO: query
    // TODO: call
}

impl<T: State + Describe + 'static> Inspect for T {
    fn describe(&self) -> Descriptor {
        Self::describe()
    }

    fn attach(&mut self, store: Store) -> Result<()> {
        State::attach(self, store)
    }

    fn state_version(&self) -> u32 {
        0 // TODO
    }

    fn to_any(&self) -> Result<Box<dyn Any>> {
        todo!()
        // let bytes = self.encode()?;
        // let cloned = Self::decode(bytes.as_slice())?;
        // Ok(Box::new(cloned))
    }
}

trait MaybeDisplay {
    fn maybe_to_string(&self) -> Option<String>;
}

struct DisplayWrapper<'a, T>(&'a T);

impl<'a, T> MaybeDisplay for DisplayWrapper<'a, T> {
    default fn maybe_to_string(&self) -> Option<String> {
        None
    }
}

impl<'a, T: Display> MaybeDisplay for DisplayWrapper<'a, T> {
    fn maybe_to_string(&self) -> Option<String> {
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

trait MaybeToJson {
    fn maybe_to_json(&self) -> Result<Option<serde_json::Value>>;

    fn maybe_write_json<W: std::io::Write>(&self, out: W) -> Result<()>;

    fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>>;
}

type WasmResult<T> = std::result::Result<T, JsValue>;

struct ToJsonWrapper<T>(T);

impl<T> MaybeToJson for ToJsonWrapper<T> {
    default fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }

    default fn maybe_write_json<W: std::io::Write>(&self, _out: W) -> Result<()> {
        Err(Error::Downcast("Cannot write type as JSON".to_string()))
    }

    default fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>> {
        Ok(None)
    }
}

impl<T: Serialize> MaybeToJson for ToJsonWrapper<T> {
    fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        Ok(Some(serde_json::to_value(&self.0)?))
    }

    fn maybe_write_json<W: std::io::Write>(&self, out: W) -> Result<()> {
        Ok(serde_json::to_writer(out, &self.0)?)
    }

    fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>> {
        Ok(Some(serde_wasm_bindgen::to_value(&self.0)?))
    }
}

pub fn err_to_js<E: std::error::Error>(err: E) -> JsValue {
    js_sys::Error::new(err.to_string().as_str()).into()
}

#[wasm_bindgen]
pub struct JsIter(Box<dyn Iterator<Item = WasmResult<JsValue>>>);

impl JsIter {
    pub fn new<T>(iter: T) -> Self
    where
        T: Iterator<Item = WasmResult<JsValue>> + 'static,
    {
        Self(Box::new(iter))
    }
}

#[wasm_bindgen]
impl JsIter {
    #[wasm_bindgen(js_name = next)]
    pub fn next_js(&mut self) -> WasmResult<JsIterNext> {
        let next = self.0.next();

        Ok(JsIterNext {
            done: next.is_none(),
            value: next.transpose()?,
        })
    }
}

#[wasm_bindgen]
pub struct JsIterNext {
    pub done: bool,
    value: Option<JsValue>,
}

#[wasm_bindgen]
impl JsIterNext {
    #[wasm_bindgen(getter)]
    pub fn value(&mut self) -> JsValue {
        self.value.take().unwrap_or_default()
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

impl<T: 'static> Describe for std::marker::PhantomData<T> {
    fn describe() -> Descriptor {
        Builder::new::<Self>().build()
    }
}

impl<T> Describe for std::cell::RefCell<T>
where
    T: State + Describe + 'static,
{
    fn describe() -> Descriptor {
        Builder::new::<Self>()
            .named_child::<T>("inner", &[])
            .build()
    }
}

impl<T, const N: usize> Describe for [T; N]
where
    T: State + Describe + Terminated + 'static,
{
    fn describe() -> Descriptor {
        // TODO: add child descriptors
        Builder::new::<Self>().build()
    }
}

impl<T> Describe for Vec<T>
where
    T: State + Describe + Terminated + 'static,
{
    fn describe() -> Descriptor {
        // TODO: add child descriptors
        Builder::new::<Self>().build()
    }
}

impl<T> Describe for Option<T>
where
    T: State + Describe + 'static,
{
    fn describe() -> Descriptor {
        Builder::new::<Self>()
            // .named_child::<T>("inner", &[], |v| Builder::maybe_access(v, Self::take))
            .build()
    }
}

macro_rules! tuple_impl {
    ($($type:ident),*; $last_type:ident; $($indices:tt),*; $last_index:tt) => {
        impl<$($type,)* $last_type> Describe for ($($type,)* $last_type)
        where
            $($type: State + Encode + Decode + Terminated + Describe + 'static,)*
            $last_type: State + Encode + Decode + Describe + 'static,
        {
            fn describe() -> Descriptor {
                // TODO: add child descriptors
                Builder::new::<Self>()
                    $(.named_child::<$type>(stringify!($indices), &[$indices as u8]))*
                    .named_child::<$last_type>(stringify!($last_index), &[$last_index as u8])
                    .build()
            }
        }
    }
}

tuple_impl!(A; B; 0; 1);
tuple_impl!(A, B; C; 0, 1; 2);
tuple_impl!(A, B, C; D; 0, 1, 2; 3);
tuple_impl!(A, B, C, D; E; 0, 1, 2, 3; 4);
tuple_impl!(A, B, C, D, E; F; 0, 1, 2, 3, 4; 5);
tuple_impl!(A, B, C, D, E, F; G; 0, 1, 2, 3, 4, 5; 6);
tuple_impl!(A, B, C, D, E, F, G; H; 0, 1, 2, 3, 4, 5, 6; 7);
tuple_impl!(A, B, C, D, E, F, G, H; I; 0, 1, 2, 3, 4, 5, 6, 7; 8);
tuple_impl!(A, B, C, D, E, F, G, H, I; J; 0, 1, 2, 3, 4, 5, 6, 7, 8; 9);
tuple_impl!(A, B, C, D, E, F, G, H, I, J; K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9; 10);
tuple_impl!(A, B, C, D, E, F, G, H, I, J, K; L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10; 11);

// #[cfg(test)]
// mod tests {
//     use serde::{Deserialize, Serialize};

//     use super::{Describe, Value};
//     use crate::{
//         collections::Map,
//         encoding::{Decode, Encode},
//         state::State,
//         store::{DefaultBackingStore, MapStore, Shared, Store},
//     };

//     #[derive(State, Encode, Decode, Describe, Debug, Serialize, Deserialize, PartialEq)]
//     struct Foo {
//         bar: u32,
//         baz: u32,
//     }

//     #[derive(State, Encode, Decode, Describe, Default)]
//     struct Bar {
//         bar: u32,
//         baz: Map<u32, u32>,
//     }

//     #[derive(State, Encode, Decode, Describe, Default)]
//     struct Baz<T: State>(Map<u32, T>);

//     fn create_bar_value() -> Value {
//         let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));

//         let mut bar = Bar::default();
//         bar.attach(store.clone()).unwrap();

//         bar.baz.insert(123, 456).unwrap();
//         bar.baz.insert(789, 1).unwrap();
//         bar.baz.insert(1000, 2).unwrap();
//         bar.baz.insert(1001, 3).unwrap();
//         bar.flush().unwrap();

//         let mut value = Value::new(bar);
//         value.attach(store).unwrap();

//         value
//     }

//     #[test]
//     fn decode() {
//         let desc = Foo::describe();
//         let value = desc.decode(&[0, 0, 1, 164, 0, 0, 0, 69]).unwrap();
//         assert_eq!(
//             value.maybe_debug(false).unwrap(),
//             "Foo { bar: 420, baz: 69 }"
//         );
//     }

//     #[test]
//     fn downcast() {
//         let value = Value::new(Foo { bar: 420, baz: 69 });
//         let foo: Foo = value.downcast().unwrap();
//         assert_eq!(foo.bar, 420);
//         assert_eq!(foo.baz, 69);
//     }

//     #[test]
//     fn child() {
//         let value = Value::new(Foo { bar: 420, baz: 69 });
//         let bar: u32 = value.child("bar").unwrap().unwrap().downcast().unwrap();
//         let baz: u32 = value.child("baz").unwrap().unwrap().downcast().unwrap();
//         assert_eq!(bar, 420);
//         assert_eq!(baz, 69);
//     }

//     #[test]
//     fn complex_child() {
//         let value = create_bar_value();

//         let baz = value.child("baz").unwrap().unwrap();
//         assert_eq!(
//             baz.child("123")
//                 .unwrap()
//                 .unwrap()
//                 .downcast::<u32>()
//                 .unwrap(),
//             456
//         );
//     }

//     #[test]
//     fn json() {
//         let value = Value::new(Foo { bar: 420, baz: 69 });
//         assert_eq!(
//             value.maybe_to_json().unwrap().unwrap().to_string(),
//             "{\"bar\":420,\"baz\":69}".to_string(),
//         );
//         #[cfg(target_arch = "wasm32")]
//         assert_eq!(
//             serde_wasm_bindgen::from_value::<Foo>(value.maybe_to_wasm().unwrap().unwrap()).unwrap(),
//             Foo { bar: 420, baz: 69 },
//         );

//         let value = Value::new(Bar::default());
//         assert!(value.maybe_to_json().unwrap().is_none());
//         #[cfg(target_arch = "wasm32")]
//         assert!(value.maybe_to_wasm().unwrap().is_none());
//     }

//     #[test]
//     fn entries() {
//         let bar = create_bar_value();
//         assert!(bar.entries().is_none());

//         let map = bar.child("baz").unwrap().unwrap();
//         let mut iter = map.entries().unwrap();
//         let mut assert_entry = |expected_key, expected_value| {
//             let (actual_key, actual_value) = iter.next().unwrap().unwrap();
//             assert_eq!(actual_key.downcast::<u32>().unwrap(), expected_key);
//             assert_eq!(actual_value.downcast::<u32>().unwrap(), expected_value);
//         };
//         assert_entry(123, 456);
//         assert_entry(789, 1);
//         assert_entry(1000, 2);
//         assert_entry(1001, 3);
//         assert!(iter.next().is_none());
//     }

//     #[test]
//     fn descriptor_json() {
//         assert_eq!(
//             serde_json::to_string(&<Bar as Describe>::describe()).unwrap(),
//             "{\"type_name\":\"orga::describe::tests::Bar\",\"state_version\":0,\"children\":{\"Named\":[{\"name\":\"bar\",\"desc\":{\"type_name\":\"u32\",\"state_version\":0,\"children\":\"None\"},\"store_key\":{\"Append\":[0]}},{\"name\":\"baz\",\"desc\":{\"type_name\":\"orga::collections::map::Map<u32, u32>\",\"state_version\":0,\"children\":{\"Dynamic\":{\"key_desc\":{\"type_name\":\"u32\",\"state_version\":0,\"children\":\"None\"},\"value_desc\":{\"type_name\":\"u32\",\"state_version\":0,\"children\":\"None\"}}}},\"store_key\":{\"Append\":[1]}}]}}"
//         );
//     }
// }
