use crate::{
    encoding::{Decode, Encode},
    state::State,
    store::{DefaultBackingStore, Iter, Read, Store},
    Error, Result,
};
use ed::Terminated;
use js_sys::{Array, Uint8Array};
use serde::Serialize;
use std::{
    any::Any,
    fmt::{Debug, Display},
    ops::Deref,
};
use wasm_bindgen::prelude::*;

mod builder;

pub use crate::macros::Describe;
pub use builder::Builder;

pub trait Describe {
    fn describe() -> Descriptor;
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct Descriptor {
    pub type_name: String,
    children: Children,
    decode: DecodeFn,
    parse: ParseFn,
}

impl Debug for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Descriptor")
            .field("type_name", &self.type_name)
            .field("children", &self.children)
            .finish()
    }
}

impl Descriptor {
    pub fn decode(&self, bytes: &[u8]) -> Result<Value> {
        (self.decode)(bytes)
    }

    pub fn from_str(&self, string: &str) -> Result<Option<Value>> {
        (self.parse)(string)
    }

    pub fn children(&self) -> &Children {
        &self.children
    }
}

#[wasm_bindgen]
impl Descriptor {
    #[wasm_bindgen(js_name = children)]
    pub fn children_js(&self) -> JsValue {
        match &self.children {
            Children::None => JsValue::NULL,
            Children::Named(children) => children
                .iter()
                .cloned()
                .map(JsValue::from)
                .collect::<Array>()
                .into(),
            Children::Dynamic(child) => child.clone().into(),
        }
    }

    #[wasm_bindgen(js_name = decode)]
    pub fn decode_js(&self, bytes: js_sys::Uint8Array) -> Value {
        // TODO: return Result
        self.decode(bytes.to_vec().as_slice()).unwrap()
    }
}

pub type DecodeFn = fn(&[u8]) -> Result<Value>;
pub type ParseFn = fn(&str) -> Result<Option<Value>>;

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

impl Children {
    fn get_dynamic_child(&self, key_bytes: Vec<u8>, store: &Store) -> Result<Value> {
        let value_desc = match self {
            Children::Dynamic(desc) => &desc.value_desc,
            _ => {
                return Err(Error::Downcast(
                    "Value does not have dynamic children".to_string(),
                ))
            }
        };

        let value_bytes = store
            .get(key_bytes.as_slice())?
            .ok_or_else(|| Error::Store("Value not found".to_string()))?;

        let substore = store.sub(&key_bytes);
        let mut value = value_desc.decode(value_bytes.as_slice())?;
        value.attach(substore)?;

        Ok(value)
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
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

#[wasm_bindgen(inspectable)]
#[derive(Clone, Debug)]
pub struct DynamicChild {
    key_desc: Box<Descriptor>,
    value_desc: Box<Descriptor>,
}

impl DynamicChild {
    pub fn key_desc(&self) -> &Descriptor {
        &self.key_desc
    }

    pub fn value_desc(&self) -> &Descriptor {
        &self.value_desc
    }
}

#[derive(Clone, Debug)]
pub enum KeyOp {
    Append(Vec<u8>),
    Absolute(Vec<u8>),
}

impl KeyOp {
    pub fn apply(&self, store: &Store) -> Store {
        match self {
            KeyOp::Absolute(prefix) => unsafe { store.with_prefix(prefix.clone()) },
            KeyOp::Append(prefix) => store.sub(prefix.as_slice()),
        }
    }
}

#[wasm_bindgen]
pub struct WrappedStore(Store);

#[wasm_bindgen(inspectable)]
pub struct Value {
    instance: Box<dyn Inspect>,
    store: Store,
}

impl Value {
    pub fn new<T: Inspect + 'static>(instance: T) -> Self {
        Value {
            instance: Box::new(instance),
            store: Store::default(),
        }
    }

    pub fn attach(&mut self, store: Store) -> Result<()> {
        self.store = store.clone();
        self.instance.attach(store)
    }

    pub fn downcast<T: Inspect + 'static>(&self) -> Option<T> {
        let any = self.instance.to_any().unwrap();
        match any.downcast::<T>() {
            Ok(mut boxed) => {
                // TODO: return Result
                boxed.attach(self.store.clone()).unwrap();
                Some(*boxed)
            }
            Err(_) => None,
        }
    }

    pub fn child(&self, name: &str) -> Result<Value> {
        let desc = self.describe();
        match desc.children {
            Children::None => Err(Error::Downcast("Value does not have children".to_string())),
            Children::Named(children) => {
                let cdesc = children
                    .iter()
                    .find(|c| c.name == name)
                    .ok_or_else(|| Error::Downcast(format!("No child called '{}'", name)))?;

                let substore = cdesc.store_key.apply(&self.store);
                let mut child = (cdesc.access)(self)?;
                child.attach(substore)?;
                Ok(child)
            }
            Children::Dynamic(ref child) => {
                let key_bytes = child
                    .key_desc
                    .from_str(name)?
                    .ok_or_else(|| {
                        Error::Downcast(
                            "Dynamic child key can not be parsed from string".to_string(),
                        )
                    })?
                    .encode()?;
                desc.children.get_dynamic_child(key_bytes, &self.store)
            }
        }
    }

    pub fn entries(&self) -> Option<EntryIter> {
        let desc = self.describe();
        match desc.children {
            Children::Dynamic(kv_desc) => Some(EntryIter {
                kv_desc,
                store_iter: self.store.range(..),
            }),
            _ => None,
        }
    }

    pub fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        self.instance.maybe_to_json()
    }

    pub fn to_json(&self) -> Result<serde_json::Value> {
        self.maybe_to_json()?
            .ok_or_else(|| Error::App(format!("Could not convert '{}' to JSON", self.type_name())))
    }

    pub fn type_name(&self) -> String {
        self.describe().type_name
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}

#[wasm_bindgen]
impl Value {
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> Option<String> {
        self.maybe_to_string()
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json_js(&self) -> WasmResult<JsValue> {
        self.maybe_to_wasm().map(|opt| opt.unwrap_or(JsValue::NULL))
    }

    #[wasm_bindgen(js_name = debug)]
    pub fn maybe_debug_js(&self, alternate: Option<bool>) -> Option<String> {
        let alternate = alternate.unwrap_or_default();
        self.maybe_debug(alternate)
    }

    #[wasm_bindgen(js_name = child)]
    pub fn child_js(&self, name: &str) -> Value {
        // TODO: return Result
        self.child(name).unwrap()
    }

    #[wasm_bindgen(js_name = encode)]
    pub fn encode_js(&self) -> Uint8Array {
        // TODO: return Result
        self.encode().unwrap().as_slice().into()
    }

    #[wasm_bindgen(js_name = entries)]
    pub fn entries_js(&self) -> JsValue {
        // TODO: needs to return object with Symbol.iterator property
        self.entries()
            .map(|iter| {
                JsIter::new(iter.map(|res| match res {
                    Ok(kv) => {
                        let arr = js_sys::Array::new();
                        arr.push(&kv.0.into());
                        arr.push(&kv.1.into());
                        Ok(arr.into())
                    }
                    Err(err) => Err(err_to_js(err)),
                }))
                .into()
            })
            .unwrap_or(JsValue::NULL)
    }
}

impl Deref for Value {
    type Target = dyn Inspect;

    fn deref(&self) -> &Self::Target {
        &*self.instance
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

    fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>> {
        MaybeToJson::maybe_to_wasm(&ToJsonWrapper(&self))
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

    fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>>;
}

type WasmResult<T> = std::result::Result<T, JsValue>;

struct ToJsonWrapper<T>(T);

impl<T> MaybeToJson for ToJsonWrapper<T> {
    default fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }

    default fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>> {
        Ok(None)
    }
}

impl<T: Serialize> MaybeToJson for ToJsonWrapper<T> {
    fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        Ok(Some(serde_json::to_value(&self.0)?))
    }

    fn maybe_to_wasm(&self) -> WasmResult<Option<JsValue>> {
        Ok(Some(serde_wasm_bindgen::to_value(&self.0)?))
    }
}

pub struct EntryIter {
    kv_desc: DynamicChild,
    store_iter: Iter<Store>,
}

impl Iterator for EntryIter {
    type Item = Result<(Value, Value)>;

    fn next(&mut self) -> Option<Result<(Value, Value)>> {
        self.store_iter.next().map(|res| {
            res.map(|(key_bytes, value_bytes)| {
                let key = self.kv_desc.key_desc.decode(&key_bytes)?;
                let value = self.kv_desc.value_desc.decode(&value_bytes)?;
                Ok((key, value))
            })?
        })
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
    #[wasm_bindgen(method, getter)]
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
    T: State + Encode + Decode + Describe + 'static,
{
    fn describe() -> Descriptor {
        Builder::new::<Self>()
            .named_child::<T>("inner", &[], |v| {
                Builder::access(v, |v: Self| v.into_inner())
            })
            .build()
    }
}

impl<T, const N: usize> Describe for [T; N]
where
    T: State + Encode + Decode + Terminated + Describe + 'static,
{
    fn describe() -> Descriptor {
        // TODO: add child descriptors
        Builder::new::<Self>().build()
    }
}

impl<T> Describe for Vec<T>
where
    T: State + Encode + Decode + Terminated + Describe + 'static,
{
    fn describe() -> Descriptor {
        // TODO: add child descriptors
        Builder::new::<Self>().build()
    }
}

impl<T> Describe for Option<T>
where
    T: State + Encode + Decode + Terminated + Describe + 'static,
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
                    $(.named_child::<$type>(stringify!($indices), &[$indices as u8], |v| Builder::access(v, |v: Self| v.$indices)))*
                    .named_child::<$last_type>(stringify!($last_index), &[$last_index as u8], |v| Builder::access(v, |v: Self| v.$last_index))
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

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::{Builder, Describe, Descriptor, Value};
    use crate::{
        collections::Map,
        encoding::{Decode, Encode},
        state::State,
        store::{DefaultBackingStore, MapStore, Shared, Store},
    };

    #[derive(State, Encode, Decode, Describe, Debug, Serialize, Deserialize, PartialEq)]
    struct Foo {
        bar: u32,
        baz: u32,
    }

    #[derive(State, Encode, Decode, Describe, Default)]
    struct Bar {
        bar: u32,
        baz: Map<u32, u32>,
    }

    #[derive(State, Encode, Decode, Describe, Default)]
    struct Baz<T: State>(Map<u32, T>);

    fn create_bar_value() -> Value {
        let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));

        let mut bar = Bar::default();
        bar.attach(store.clone()).unwrap();

        bar.baz.insert(123, 456).unwrap();
        bar.baz.insert(789, 1).unwrap();
        bar.baz.insert(1000, 2).unwrap();
        bar.baz.insert(1001, 3).unwrap();
        bar.flush().unwrap();

        let mut value = Value::new(bar);
        value.attach(store).unwrap();

        value
    }

    #[test]
    fn decode() {
        let desc = Foo::describe();
        let value = desc.decode(&[0, 0, 1, 164, 0, 0, 0, 69]).unwrap();
        assert_eq!(
            value.maybe_debug(false).unwrap(),
            "Foo { bar: 420, baz: 69 }"
        );
    }

    #[test]
    fn downcast() {
        let value = Value::new(Foo { bar: 420, baz: 69 });
        let foo: Foo = value.downcast().unwrap();
        assert_eq!(foo.bar, 420);
        assert_eq!(foo.baz, 69);
    }

    #[test]
    fn child() {
        let value = Value::new(Foo { bar: 420, baz: 69 });
        let bar: u32 = value.child("bar").unwrap().downcast().unwrap();
        let baz: u32 = value.child("baz").unwrap().downcast().unwrap();
        assert_eq!(bar, 420);
        assert_eq!(baz, 69);
    }

    #[test]
    fn complex_child() {
        let mut value = create_bar_value();

        let baz = value.child("baz").unwrap();
        assert_eq!(baz.child("123").unwrap().downcast::<u32>().unwrap(), 456);
    }

    #[test]
    fn json() {
        let value = Value::new(Foo { bar: 420, baz: 69 });
        assert_eq!(
            value.maybe_to_json().unwrap().unwrap().to_string(),
            "{\"bar\":420,\"baz\":69}".to_string(),
        );
        #[cfg(target_arch = "wasm32")]
        assert_eq!(
            serde_wasm_bindgen::from_value::<Foo>(value.maybe_to_wasm().unwrap().unwrap()).unwrap(),
            Foo { bar: 420, baz: 69 },
        );

        let value = Value::new(Bar::default());
        assert!(value.maybe_to_json().unwrap().is_none());
        #[cfg(target_arch = "wasm32")]
        assert!(value.maybe_to_wasm().unwrap().is_none());
    }

    #[test]
    fn entries() {
        let bar = create_bar_value();
        assert!(bar.entries().is_none());

        let map = bar.child("baz").unwrap();
        let mut iter = map.entries().unwrap();
        let mut assert_entry = |expected_key, expected_value| {
            let (actual_key, actual_value) = iter.next().unwrap().unwrap();
            assert_eq!(actual_key.downcast::<u32>().unwrap(), expected_key);
            assert_eq!(actual_value.downcast::<u32>().unwrap(), expected_value);
        };
        assert_entry(123, 456);
        assert_entry(789, 1);
        assert_entry(1000, 2);
        assert_entry(1001, 3);
        assert!(iter.next().is_none());
    }
}
