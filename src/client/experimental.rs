use crate::call::Call;
use crate::describe::{Children, Describe, Descriptor, KeyOp};
use crate::merk::BackingStore;
use crate::prelude::Shared;
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use std::any::TypeId;
use std::cell::RefCell;

#[derive(Debug, Clone)]
struct Trace {
    type_id: TypeId,
    method_prefix: Vec<u8>,
    method_args: Vec<u8>,
}

impl Trace {
    pub fn bytes(&self) -> Vec<u8> {
        vec![self.method_prefix.clone(), self.method_args.clone()].concat()
    }
}

thread_local! {
    static TRACE: RefCell<Option<Trace>> = RefCell::new(None);
}

pub fn trace<T: 'static>(method_prefix: Vec<u8>, method_args: Vec<u8>) -> Result<()> {
    let type_id = TypeId::of::<T>();
    TRACE.with(|trace| {
        let mut trace = trace
            .try_borrow_mut()
            .map_err(|_| Error::Call("Call tracer is already borrowed".to_string()))?;
        trace.replace(Trace {
            type_id,
            method_prefix,
            method_args,
        });
        Result::Ok(())
    })
}

fn take_trace() -> Option<Trace> {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.take()
    })
}

pub struct Client<T> {
    descriptor: Descriptor,
    _pd: std::marker::PhantomData<T>,
}

impl<T> Client<T> {
    pub fn new() -> Self
    where
        T: Describe,
    {
        Self {
            descriptor: T::describe(),
            _pd: std::marker::PhantomData,
        }
    }
}

impl<T> Client<T> {
    pub fn call<F: Fn(&mut T) -> Result<()>>(&mut self, call_fn: F) -> Result<Vec<u8>>
    where
        T: Call + Default + State,
    {
        dbg!(&self.descriptor);
        let mut value = T::default();
        let mut call_store = Store::new(BackingStore::Other(Shared::new(Box::new(CallStore {}))));
        let map_store = Store::with_map_store();
        value.attach(map_store.clone())?;
        let mut old_bytes = vec![];
        value.flush(&mut old_bytes)?;

        let mut value = T::load(map_store, &mut old_bytes.as_slice())?;
        value.attach(call_store.clone())?;
        match call_fn(&mut value) {
            Err(Error::ClientStore { mut store_op }) => {
                store_op.old_value.replace(old_bytes.clone());
                let trace = take_trace().unwrap();
                let receiver =
                    self.descriptor
                        .resolve_by_type_id(trace.type_id, store_op, vec![])?;
                return Ok([receiver, trace.bytes()].concat());
            }
            Ok(_) => {}
            // _ => todo!(),
            _ => {}
        }
        let mut bytes = vec![];
        match value.flush(&mut bytes) {
            Err(Error::ClientStore { mut store_op }) => {
                dbg!(&store_op);
                store_op.old_value.replace(old_bytes.clone());
                let trace = take_trace().unwrap();
                let receiver =
                    self.descriptor
                        .resolve_by_type_id(trace.type_id, store_op, vec![])?;
                return Ok([receiver, trace.bytes()].concat());
            }
            Ok(_) => {}
            _ => todo!(),
        }

        match call_store.put(vec![], bytes) {
            Err(Error::ClientStore { mut store_op }) => {
                dbg!(&store_op);
                store_op.old_value.replace(old_bytes.clone());
                let trace = take_trace().unwrap();
                let receiver =
                    self.descriptor
                        .resolve_by_type_id(trace.type_id, store_op, vec![])?;
                return Ok([receiver, trace.bytes()].concat());
            }
            Ok(_) => {
                panic!("did not write")
            }
            _ => todo!(),
        }
    }
}

pub struct CallStore {}

use crate::store::{Read, Write};

impl Read for CallStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let store_op = StoreOp {
            key: key.to_vec(),
            old_value: None,
            new_value: None,
        };
        Err(Error::ClientStore { store_op })
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<crate::prelude::KV>> {
        let store_op = StoreOp {
            key: key.to_vec(),
            old_value: None,
            new_value: None,
        };
        Err(Error::ClientStore { store_op })
    }
}

impl Write for CallStore {
    fn delete(&mut self, key: &[u8]) -> Result<()> {
        let store_op = StoreOp {
            key: key.to_vec(),
            old_value: None, // TODO: get old value from internal store
            new_value: None,
        };
        Err(Error::ClientStore { store_op })
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let store_op = StoreOp {
            key,
            old_value: None, // TODO: get old value from internal store
            new_value: Some(value),
        };
        Err(Error::ClientStore { store_op })
    }
}

#[derive(Debug, Clone)]
pub struct StoreOp {
    key: Vec<u8>,
    old_value: Option<Vec<u8>>,
    new_value: Option<Vec<u8>>,
}

impl Descriptor {
    pub fn resolve_by_type_id(
        &self,
        target_type_id: TypeId,
        mut store_op: StoreOp,
        mut self_store_key: Vec<u8>,
    ) -> Result<Vec<u8>> {
        if self.type_id == target_type_id {
            return Ok(self_store_key);
        }

        if self_store_key.len() == store_op.key.len() {
            let old_value = store_op.old_value.as_mut().unwrap();
            let new_value = store_op.new_value.as_mut().unwrap();
            if let Some(ref meta) = self.meta {
                let consumed = meta.encoding_bytes_subslice(new_value)?;
                *new_value = new_value[consumed.len()..].to_vec();
                let consumed = meta.encoding_bytes_subslice(old_value)?;
                *old_value = old_value[consumed.len()..].to_vec();
            }

            if let Children::Named(children) = self.children() {
                for child in children {
                    let consumed = child.desc.encoding_bytes_subslice(new_value)?;
                    if old_value.len() < consumed.len() || consumed != &old_value[..consumed.len()]
                    {
                        return child.desc.resolve_by_type_id(
                            target_type_id,
                            store_op,
                            child.store_key.apply_bytes(self_store_key.as_slice()),
                        );
                    }
                    let consumed_len = consumed.len();
                    *new_value = new_value[consumed_len..].to_vec();
                    *old_value = old_value[consumed_len..].to_vec();
                }
            }

            return Err(Error::Client("No matching child".to_string()));
        }

        let child_key = &store_op.key[self_store_key.len()..];
        match self.children() {
            Children::None => Err(Error::Client("No matching child".to_string())),
            Children::Named(children) => {
                for child in children {
                    use KeyOp::*;
                    match child.store_key {
                        Append(ref bytes) => {
                            if child_key.starts_with(bytes) {
                                return child.desc.resolve_by_type_id(
                                    target_type_id,
                                    store_op,
                                    child.store_key.apply_bytes(self_store_key.as_slice()),
                                );
                            }
                        }
                        _ => continue,
                    }
                }
                Err(Error::Client("No matching child".to_string()))
            }
            Children::Dynamic(child) => {
                let consumed = child.key_desc().encoding_bytes_subslice(child_key)?;
                self_store_key.extend_from_slice(consumed);
                child
                    .value_desc()
                    .resolve_by_type_id(target_type_id, store_op, self_store_key)
            }
        }
    }

    pub fn encoding_bytes_subslice<'a>(&self, bytes: &'a [u8]) -> Result<&'a [u8]> {
        let store = Store::default();
        let mut consume_bytes = &*bytes;
        if let Some(load) = self.load {
            load(store, &mut consume_bytes)?;
            Ok(&bytes[..bytes.len() - consume_bytes.len()])
        } else {
            Err(Error::Client("No load function".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    use crate::coins::Symbol;
    use crate::collections::Map;
    use crate::encoding::Decode;
    use crate::orga;

    #[orga]
    #[derive(Debug)]
    pub struct Bar {
        pub a: u64,
        pub b: u64,
        pub c: Map<u32, u64>,
    }

    #[orga]
    impl Bar {
        #[call]
        pub fn inc_b(&mut self, n: u64) -> Result<()> {
            self.b += n;
            Ok(())
        }

        #[call]
        pub fn insert_into_map(&mut self, key: u32, value: u64) -> Result<()> {
            self.c.insert(key, value)
        }
    }

    #[orga]
    pub struct Foo {
        #[call]
        pub my_field: u32,

        #[call]
        pub b: u64,

        pub c: u8,

        pub d: u64,

        pub e: Map<u32, Bar>,

        #[call]
        #[state(prefix(17))]
        pub bar: Bar,

        #[call]
        pub staking: crate::coins::Staking<Simp>,
    }
    #[orga]
    #[derive(Clone, Debug)]
    pub struct Simp {}
    impl Symbol for Simp {
        const INDEX: u8 = 12;
    }

    #[orga]
    impl Foo {
        #[call]
        pub fn my_method(&mut self, a: u32, b: u8, c: u16) -> Result<()> {
            Ok(())
        }

        #[call]
        pub fn my_other_method(&mut self, d: u32) -> Result<()> {
            println!("called my_other_method({})", d);
            Ok(())
        }
    }

    #[test]
    #[serial]
    fn basic_call_client() -> Result<()> {
        let mut client = Client::<Foo>::new();

        let call_bytes = client.call(|foo| foo.bar.insert_into_map(6, 14))?;

        assert_eq!(
            call_bytes.as_slice(),
            &[17, 65, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 14]
        );
        let _call = <Foo as Call>::Call::decode(call_bytes.as_slice())?;

        Ok(())
    }

    #[test]
    fn resolve_child_encoding() -> Result<()> {
        let desc = Foo::describe();
        let foo = Foo::default();
        let mut bytes_before = vec![];
        foo.flush(&mut bytes_before)?;

        let mut foo = Foo::default();
        foo.d = 42;
        let mut bytes_after = vec![];
        foo.flush(&mut bytes_after)?;

        let store_op = StoreOp {
            key: vec![],
            old_value: Some(bytes_before),
            new_value: Some(bytes_after),
        };

        let tid = TypeId::of::<u64>();
        let store_key = desc.resolve_by_type_id(tid, store_op, vec![])?;

        assert_eq!(store_key, vec![3]);

        Ok(())
    }

    #[test]
    fn dynamic_child() -> Result<()> {
        let desc = Foo::describe();
        let bar = Bar::default();
        let mut bytes_before = vec![];
        bar.flush(&mut bytes_before)?;

        let mut bar = Bar::default();
        bar.b = 42;
        let mut bytes_after = vec![];
        bar.flush(&mut bytes_after)?;

        let store_op = StoreOp {
            key: vec![4, 0, 0, 0, 7],
            old_value: Some(bytes_before),
            new_value: Some(bytes_after),
        };

        let tid = TypeId::of::<u64>();
        let store_key = desc.resolve_by_type_id(tid, store_op, vec![])?;

        assert_eq!(store_key, vec![4, 0, 0, 0, 7, 1]);

        Ok(())
    }
}
