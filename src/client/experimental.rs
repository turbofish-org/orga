use crate::call::Call;
use crate::coins::{Address, Symbol};
use crate::describe::{Children, Describe, Descriptor, KeyOp};
use crate::encoding::{Decode, Encode};
use crate::merk::BackingStore;
use crate::plugins::{PaidCall, PayableCall};
use crate::prelude::Shared;
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::store::{Read, Write};
use crate::{Error, Result};
use educe::Educe;
use std::any::TypeId;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::Arc;

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
    static TRACE: RefCell<Vec<Trace>> = RefCell::new(vec![]);
}

pub fn trace<T: 'static>(method_prefix: Vec<u8>, method_args: Vec<u8>) -> Result<()> {
    let type_id = TypeId::of::<T>();
    TRACE.with(|traces| {
        let mut traces = traces
            .try_borrow_mut()
            .map_err(|_| Error::Call("Call tracer is already borrowed".to_string()))?;

        traces.push(Trace {
            type_id,
            method_prefix,
            method_args,
        });
        Result::Ok(())
    })
}

pub fn with_trace<F: FnOnce() -> std::result::Result<T, E>, T, E>(
    op: F,
) -> std::result::Result<T, E> {
    let res = op();
    if res.is_ok() {
        TRACE.with(|traces| {
            let mut traces = traces.try_borrow_mut().unwrap(); // TODO
            traces.pop();
        })
    }
    res
}

fn take_trace() -> Vec<Trace> {
    TRACE.take()
}

#[derive(Educe)]
#[educe(Clone)]
pub struct Client<T, S> {
    descriptor: Descriptor,
    store_client: Arc<S>,
    _pd: std::marker::PhantomData<T>,
}

impl<T, S> Client<T, S> {
    pub fn new(store_client: S) -> Self
    where
        T: Describe + Call + Query + State,
        S: StoreClient,
    {
        Self {
            descriptor: T::describe(),
            store_client: Arc::new(store_client),
            _pd: std::marker::PhantomData,
        }
    }

    pub fn call<F>(&self, op: F) -> Result<()>
    where
        T: State + Call + Describe + Default,
        F: Fn(&T) -> T::Call,
        S: StoreClient,
    {
        let call = op(&T::default());
        let call_bytes = call.encode()?;
        self.store_client.call_bytes(call_bytes.as_slice())
    }

    pub fn query<F: Fn(T) -> Result<U>, U>(&self, query_fn: F) -> Result<U>
    where
        T: State + Query + Describe,
        S: StoreClient,
    {
        let mut store = Some(Store::new(BackingStore::Other(Shared::new(Box::new(
            QueryStore {
                store: Some(Store::with_partial_map_store()),
            },
        )))));
        let mut height: Option<u64> = None;
        let _ = take_trace();
        const MAX_STEPS: usize = 1000;
        for _ in 0..MAX_STEPS {
            let step = |store: Store| -> Result<U> {
                let root_bytes = store.get(&[])?.unwrap_or_default();
                let app = T::load(store, &mut root_bytes.as_slice())?;
                query_fn(app)
            };

            let step_store = store.take().unwrap();
            let res = step(step_store.clone());
            store.replace(step_store);
            let store_op = match res {
                Ok(value) => return Ok(value),
                Err(Error::ClientStore { store_op }) => store_op,
                Err(other_err) => {
                    return Err(other_err);
                }
            };
            let traces = take_trace();

            let res = if !traces.is_empty() {
                let trace = traces[0].clone();
                let receiver_pfx = self.descriptor.resolve_by_type_id(
                    trace.type_id,
                    store_op.clone(),
                    vec![],
                    vec![],
                )?;
                let query_bytes = [receiver_pfx, trace.bytes()].concat();
                self.store_client
                    .query_bytes(query_bytes.as_slice(), &height)?
            } else {
                None
            };

            let (data, res_height) = match res {
                Some(data) => data,
                None => self
                    .store_client
                    .query_key(store_op.key.as_slice(), &height)?,
            };

            height.replace(res_height);

            let mut query_store = *store
                .take()
                .unwrap()
                .into_backing_store()
                .into_inner()
                .into_other()?
                .into_inner()
                .into_any()
                .downcast::<QueryStore>()
                .unwrap();
            query_store.add_data(data)?;
            store = Some(Store::new(BackingStore::Other(Shared::new(Box::new(
                query_store,
            )))));
        }

        Err(Error::Client("Max steps reached".into()))
    }
}

fn join_store(mut dst: Store, src: Store) -> Result<Store> {
    // TODO: join proof stores
    let src_is_map_store = true;
    if src_is_map_store {
        for entry in src.range(..) {
            let (k, v) = entry?;
            dst.put(k, v)?;
        }
    }
    Ok(dst)
}

pub trait StoreClient: Clone {
    fn query_key(&self, key: &[u8], height: &Option<u64>) -> Result<(Store, u64)>;

    fn query_bytes(&self, query_bytes: &[u8], height: &Option<u64>)
        -> Result<Option<(Store, u64)>>;

    fn call_bytes(&self, call_bytes: &[u8]) -> Result<()>;
}
pub trait Transport {}

pub struct QueryStore {
    store: Option<Store>,
}

impl QueryStore {
    pub fn add_data(&mut self, data: Store) -> Result<()> {
        if let Some(store) = self.store.take() {
            self.store.replace(join_store(store, data)?);
        } else {
            self.store.replace(data);
        }

        Ok(())
    }
}

impl Read for QueryStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(store) = &self.store {
            if let Ok(value) = store.get(key) {
                return Ok(value);
            }
        }

        let store_op = StoreOp {
            key: key.to_vec(),
            old_value: None,
            new_value: None,
        };
        Err(Error::ClientStore { store_op })
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<crate::store::KV>> {
        if let Some(store) = &self.store {
            // TODO: this may not be the correct behavior for get_next
            let next = store.get_next(key)?;
            if next.is_some() {
                return Ok(next);
            }
        }

        let store_op = StoreOp {
            key: key.to_vec(),
            old_value: None,
            new_value: None,
        };
        Err(Error::ClientStore { store_op })
    }
}

impl Write for QueryStore {
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
        store_op: StoreOp,
        mut self_store_key: Vec<u8>,
        mut out_bytes: Vec<u8>,
    ) -> Result<Vec<u8>> {
        if self.type_id == target_type_id {
            return Ok(out_bytes);
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
                                    child.store_key.apply_bytes(out_bytes.as_slice()),
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
                out_bytes = child.apply_query_bytes(out_bytes);
                out_bytes.extend_from_slice(consumed);
                self_store_key.extend_from_slice(consumed);
                child.value_desc().resolve_by_type_id(
                    target_type_id,
                    store_op,
                    self_store_key,
                    out_bytes,
                )
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

#[derive(Educe)]
#[educe(Clone)]
pub struct DirectStoreClient<T> {
    store: Store,
    _pd: PhantomData<T>,
}

impl<T> DirectStoreClient<T> {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            _pd: PhantomData,
        }
    }
}

impl<T> From<Store> for DirectStoreClient<T> {
    fn from(store: Store) -> Self {
        Self::new(store)
    }
}

impl<T> StoreClient for DirectStoreClient<T>
where
    T: Call + State + Query,
{
    fn query_key(&self, key: &[u8], _height: &Option<u64>) -> Result<(Store, u64)> {
        let mut res_store = Store::with_map_store();
        let value = self.store.get(key)?;
        if let Some(value) = value {
            res_store.put(key.to_vec(), value)?;
        }

        Ok((res_store, 0))
    }

    fn query_bytes(
        &self,
        query_bytes: &[u8],
        _height: &Option<u64>,
    ) -> Result<Option<(Store, u64)>> {
        dbg!("decoding query..");
        let _query = <T as Query>::Query::decode(query_bytes)?;
        dbg!(_query);
        Ok(None)
    }

    fn call_bytes(&self, call_bytes: &[u8]) -> Result<()> {
        let call = <T as Call>::Call::decode(call_bytes)?;
        let mut store = self.store.clone();
        let root_bytes = store.get(&[])?.unwrap_or_default();
        let mut app = <T as State>::load(store.clone(), &mut root_bytes.as_slice())?;
        dbg!("executing call");
        app.call(call)?;
        dbg!("executed call");
        let mut root_bytes = vec![];
        app.flush(&mut root_bytes)?;
        store.put(vec![], root_bytes)?;

        Ok(())
    }
}

#[derive(Educe)]
#[educe(Clone(bound = "W: std::clone::Clone"))]
pub struct ClientWithPlugins<T, C, S, const ID: &'static str, W> {
    _pd: PhantomData<(T, S)>,
    client: Client<DefaultPlugins<S, T, ID>, C>,
    payer_call_bytes: Option<Vec<u8>>,
    wallet: W,
}

use crate::plugins::SignerCall;
pub trait Wallet: Clone {
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall>;

    fn address(&self) -> Result<Option<Address>>;

    fn nonce(&self) -> Result<Option<u64>>;
}

#[derive(Clone, Default)]
pub struct OrgaWallet;
impl Wallet for OrgaWallet {
    fn sign(&self, call_bytes: &[u8]) -> Result<SignerCall> {
        use crate::plugins::SigType;
        Ok(SignerCall {
            call_bytes: call_bytes.to_vec(),
            signature: None,
            pubkey: None,
            sigtype: SigType::Native,
        })
    }

    fn address(&self) -> Result<Option<Address>> {
        Ok(Some(Address::from([0; 20])))
    }

    fn nonce(&self) -> Result<Option<u64>> {
        // Ok(Some(32))
        Ok(None)
    }
}

use crate::plugins::DefaultPlugins;

impl<T, C, S, const ID: &'static str, W> ClientWithPlugins<T, C, S, ID, W>
where
    W: Clone,
{
    pub fn new(client: Client<DefaultPlugins<S, T, ID>, C>, wallet: W) -> Self {
        Self {
            _pd: PhantomData,
            client,
            payer_call_bytes: None,
            wallet,
        }
    }

    pub fn pay_from<F>(&self, op: F) -> Result<Self>
    where
        F: FnOnce(&T) -> T::Call,
        T: Call + Default,
    {
        let app = &T::default();
        let payer_call = op(app);
        let payer_call_bytes = payer_call.encode()?;

        Ok(Self {
            _pd: PhantomData,
            client: self.client.clone(),
            payer_call_bytes: Some(payer_call_bytes),
            wallet: self.wallet.clone(),
        })
    }

    pub fn call<F>(&self, op: F) -> Result<()>
    where
        F: FnOnce(&T) -> T::Call,
        T: Call + Default,
        W: Wallet,
        DefaultPlugins<S, T, ID>: Call + State + Describe + Default,
        C: StoreClient,
    {
        let payer_call_bytes = self
            .payer_call_bytes
            .as_ref()
            .ok_or_else(|| Error::Client("Payer call must be provided".to_string()))?;
        let payer = <T as Call>::Call::decode(payer_call_bytes.as_slice())?;

        let app = &T::default();
        let paid = op(app);
        let call = PayableCall::Paid(PaidCall { payer, paid });
        let nonce = self.wallet.nonce()?;
        let call = crate::plugins::NonceCall {
            nonce,
            inner_call: call,
        };
        let call = [ID.as_bytes().to_vec(), call.encode()?].concat();
        let call = self.wallet.sign(&call)?;
        let call = crate::plugins::sdk_compat::Call::Native(call);
        self.client.store_client.call_bytes(&call.encode()?)?;

        Ok(())
    }

    pub fn query<F, U>(&self, op: F) -> Result<U>
    where
        F: Fn(T) -> Result<U>,
        T: Query + Describe,
        C: StoreClient,
        DefaultPlugins<S, T, ID>: Query + Describe + State,
    {
        self.client.query(|outer_app| {
            let app = outer_app.inner.inner.inner.inner.inner.inner;
            op(app)
        })
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    use crate::call::build_call;
    use crate::coins::Symbol;
    use crate::collections::{Deque, Map};
    use crate::orga;
    use crate::plugins::ConvertSdkTx;
    use crate::plugins::PaidCall;

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
            crate::plugins::disable_fee();
            self.b += n;
            Ok(())
        }

        #[call]
        pub fn insert_into_map(&mut self, key: u32, value: u64) -> Result<()> {
            self.c.insert(key, value)
        }

        #[query]
        pub fn get_from_map(&self, key: u32, offset: u32) -> Result<Option<u64>> {
            Ok(self.c.get(key + offset)?.map(|v| *v))
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
        pub deque: Deque<Map<u32, Bar>>,
        #[call]
        #[state(prefix(17))]
        pub bar: Bar,
        #[call]
        pub staking: crate::coins::Staking<Simp>,
    }

    impl ConvertSdkTx for Foo {
        type Output = PaidCall<<Self as Call>::Call>;
        fn convert(&self, _msg: &orga::prelude::sdk_compat::sdk::Tx) -> Result<Self::Output> {
            unimplemented!()
        }
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
    fn basic_client() -> Result<()> {
        let mut store = Store::with_map_store();
        let mut app = Foo::default();
        app.attach(store.clone())?;
        let mut inner_map = Map::<u32, u64>::default();
        let mut deque_inner_map = Map::<u32, Bar>::default();
        inner_map.insert(16, 32)?;
        deque_inner_map.insert(
            13,
            Bar {
                a: 3,
                b: 4,
                ..Default::default()
            },
        )?;
        app.b = 42;
        app.deque.push_back(deque_inner_map)?;
        app.e.insert(
            12,
            Bar {
                a: 1,
                b: 2,
                c: inner_map,
            },
        )?;
        app.bar.b = 8;

        let mut bytes = vec![];
        app.flush(&mut bytes)?;
        store.put(vec![], bytes)?;

        let store_client = DirectStoreClient::<Foo>::new(store.clone());
        let client = Client::<Foo, _>::new(store_client);

        let value = client.query(|app| Ok(app.b))?;
        assert_eq!(value, 42);

        let value = client.query(|app| Ok(app.e.get(12)?.unwrap().get_from_map(14, 2)?))?;
        assert_eq!(value, Some(32));

        let value = client.query(|app| Ok(app.deque.get(0)?.unwrap().get(13)?.unwrap().a))?;
        assert_eq!(value, 3);

        let bar_b = client.query(|app| Ok(app.bar.b))?;
        assert_eq!(bar_b, 8);

        let n = 5;
        client.call(|app| build_call!(app.bar.inc_b(n)))?;

        let bar_b = client.query(|app| Ok(app.bar.b))?;
        assert_eq!(bar_b, 13);

        Ok(())
    }

    #[test]
    #[serial]
    fn plugin_client() -> Result<()> {
        type App = DefaultPlugins<Simp, Foo, "myapp">;
        let mut store = Store::with_map_store();
        let mut app = App::default();
        let mut inner_app = &mut app.inner.inner.inner.inner.inner.inner;

        let mut inner_map = Map::<u32, u64>::default();
        let mut deque_inner_map = Map::<u32, Bar>::default();
        inner_map.insert(16, 32)?;
        deque_inner_map.insert(
            13,
            Bar {
                a: 3,
                b: 4,
                ..Default::default()
            },
        )?;
        inner_app.b = 42;
        inner_app.deque.push_back(deque_inner_map)?;
        inner_app.e.insert(
            12,
            Bar {
                a: 1,
                b: 2,
                c: inner_map,
            },
        )?;
        inner_app.e.insert(
            13,
            Bar {
                a: 3,
                b: 4,
                c: Default::default(),
            },
        )?;
        inner_app.bar.b = 8;
        app.attach(store.clone())?;

        let mut bytes = vec![];
        app.flush(&mut bytes)?;
        store.put(vec![], bytes)?;

        let wallet = OrgaWallet::default();
        let store_client = DirectStoreClient::<App>::new(store.clone());
        let client = Client::<App, _>::new(store_client);
        let client = ClientWithPlugins::<_, _, _, "myapp", _>::new(client, wallet);
        let bar_b = client.query(|app| Ok(app.bar.b))?;
        assert_eq!(bar_b, 8);
        let value = client.query(|app| Ok(app.e.get(12)?.unwrap().get_from_map(14, 2)?))?;
        let value = client.query(|app| {
            let x = app.e.get(12)?.unwrap();
            let y = app.e.get(13)?.unwrap();
            Ok(x.get_from_map(14, 2)?)
        })?;
        assert_eq!(value, Some(32));

        let key = 13;
        let value = client.query(|app| Ok(app.deque.get(0)?.unwrap().get(key)?.unwrap().a))?;
        assert_eq!(value, 3);
        client
            .pay_from(|app| build_call!(app.bar.inc_b(4)))?
            .call(|app| build_call!(app.my_method(1, 2, 3)))?;

        let bar_b = client.query(|app| Ok(app.bar.b))?;
        assert_eq!(bar_b, 12);

        Ok(())
    }

    // #[test]
    // #[serial]
    // fn basic_call_client() -> Result<()> {
    //     let mut client = Client::<Foo, _>::new();

    //     let call_bytes = client.call(|foo| foo.bar.insert_into_map(6, 14))?;

    //     assert_eq!(
    //         call_bytes.as_slice(),
    //         &[17, 65, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 14]
    //     );
    //     let _call = <Foo as Call>::Call::decode(call_bytes.as_slice())?;

    //     Ok(())
    // }

    // #[test]
    // fn resolve_child_encoding() -> Result<()> {
    //     let desc = Foo::describe();
    //     let foo = Foo::default();
    //     let mut bytes_before = vec![];
    //     foo.flush(&mut bytes_before)?;

    //     let mut foo = Foo::default();
    //     foo.d = 42;
    //     let mut bytes_after = vec![];
    //     foo.flush(&mut bytes_after)?;

    //     let store_op = StoreOp {
    //         key: vec![],
    //         old_value: Some(bytes_before),
    //         new_value: Some(bytes_after),
    //     };

    //     let tid = TypeId::of::<u64>();
    //     let store_key = desc.resolve_by_type_id(tid, store_op, vec![])?;

    //     assert_eq!(store_key, vec![3]);

    //     Ok(())
    // }

    // #[test]
    // fn dynamic_child() -> Result<()> {
    //     let desc = Foo::describe();
    //     let bar = Bar::default();
    //     let mut bytes_before = vec![];
    //     bar.flush(&mut bytes_before)?;

    //     let mut bar = Bar::default();
    //     bar.b = 42;
    //     let mut bytes_after = vec![];
    //     bar.flush(&mut bytes_after)?;

    //     let store_op = StoreOp {
    //         key: vec![4, 0, 0, 0, 7],
    //         old_value: Some(bytes_before),
    //         new_value: Some(bytes_after),
    //     };

    //     let tid = TypeId::of::<u64>();
    //     let store_key = desc.resolve_by_type_id(tid, store_op, vec![])?;

    //     assert_eq!(store_key, vec![4, 0, 0, 0, 7, 1]);

    //     Ok(())
    // }
}
