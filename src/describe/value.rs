use std::borrow::Borrow;
use std::cell::Cell;
use std::ops::Deref;
use std::sync::{Arc, RwLock};

use crate::describe::{err_to_js, Children, Describe, Descriptor, Inspect, JsIter, WasmResult};
use crate::encoding::{Decode, Encode, Terminated};
use crate::store::{Iter, Read, Store};
use crate::{Error, Result};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

use super::{DynamicChild, InspectRef, WithFn};

#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct Value {
    root: Arc<RwLock<dyn Inspect>>,
    store: Store,
}

impl Value {
    pub fn new<T: Inspect + 'static>(instance: T) -> Self {
        Value {
            root: Arc::new(RwLock::new(instance)),
            store: Store::default(),
        }
    }

    pub fn attach(&mut self, store: Store) -> Result<()> {
        self.store = store.clone();
        // self.instance.attach2(store)
        todo!()
    }

    pub fn with<T>(&self, mut op: impl FnMut(InspectRef) -> T) -> T {
        let self_key = self.store.prefix();
        let root_desc = self.root.read().unwrap().describe();
        let lock = self.root.read().unwrap();

        let mut output = None;
        root_desc.access_by_key(self.store.prefix(), &*lock, &mut |v| {
            output = Some(op(v));
        });

        output.unwrap()
    }

    pub fn child(&self, name: &str) -> Result<Value> {
        let desc = self.with(|v| v.describe());
        let substore = match desc.children {
            Children::None => {
                return Err(Error::Downcast("Value does not have children".to_string()))
            }
            Children::Named(children) => {
                let cdesc = children
                    .iter()
                    .find(|c| c.name == name)
                    .ok_or_else(|| Error::Downcast(format!("No child called '{}'", name)))?;

                cdesc.store_key.apply(&self.store)
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
                    .maybe_encode()
                    .ok_or(Error::Downcast(format!(
                        "Dynamic child key of type {} cannot be encoded",
                        child.key_desc.type_name
                    )))??;
                self.store.sub(&key_bytes)
            }
        };

        let mut value = self.clone();
        value.store = substore;

        Ok(value)
    }

    pub fn child_names(&self) -> Vec<String> {
        let desc = self.with(|v| v.describe());
        if let Children::Named(ref children) = desc.children() {
            children.iter().map(|c| c.name.clone()).collect()
        } else {
            vec![]
        }
    }

    pub fn entries(&self) -> Option<EntryIter> {
        let desc = self.with(|v| v.describe());
        match desc.children {
            Children::Dynamic(kv_desc) => Some(EntryIter {
                kv_desc,
                store_iter: self.store.range(..),
            }),
            _ => None,
        }
    }

    pub fn maybe_to_json(&self) -> Result<Option<serde_json::Value>> {
        // self.instance.maybe_to_json()
        todo!()
    }

    pub fn to_json(&self) -> Result<serde_json::Value> {
        self.maybe_to_json()?
            .ok_or_else(|| Error::App(format!("Could not convert '{}' to JSON", self.type_name())))
    }

    pub fn write_json<W: std::io::Write + 'static>(&self, out: W) -> Result<()> {
        // self.instance.maybe_write_json(Box::new(out))
        todo!()
    }

    pub fn type_name(&self) -> String {
        self.with(|v| v.describe().type_name)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}

#[wasm_bindgen]
impl Value {
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> Option<String> {
        // self.maybe_to_string()
        self.with(|v| v.maybe_to_string())
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json_js(&self) -> WasmResult<JsValue> {
        // self.maybe_to_wasm().map(|opt| opt.unwrap_or(JsValue::NULL))
        self.with(|v| v.maybe_to_wasm())
            .map(|opt| opt.unwrap_or(JsValue::NULL))
    }

    #[wasm_bindgen(js_name = debug)]
    pub fn maybe_debug_js(&self, alternate: Option<bool>) -> Option<String> {
        let alternate = alternate.unwrap_or_default();
        // self.maybe_debug(alternate)
        self.with(|v| v.maybe_debug(alternate))
    }

    #[wasm_bindgen(js_name = child)]
    pub fn child_js(&self, name: &str) -> Option<Value> {
        // TODO: return Result
        self.child(name).ok()
    }

    #[wasm_bindgen(js_name = childNames)]
    pub fn child_names_js(&self) -> Vec<JsValue> {
        self.child_names().iter().map(|s| s.into()).collect()
    }

    #[wasm_bindgen(js_name = encode)]
    pub fn encode_js(&self) -> Uint8Array {
        // TODO: return Result
        // self.encode().unwrap().as_slice().into()
        self.with(|v| v.maybe_encode().unwrap().unwrap().as_slice().into())
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

pub struct EntryIter {
    kv_desc: DynamicChild,
    store_iter: Iter<Store>,
}

impl Iterator for EntryIter {
    type Item = Result<(Value, Value)>;

    fn next(&mut self) -> Option<Result<(Value, Value)>> {
        todo!()
    }
}

impl Descriptor {}
