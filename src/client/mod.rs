use crate::call::Call;
use crate::coins::{Address, Symbol};
use crate::describe::{Children, Describe, Descriptor, KeyOp};
use crate::encoding::{Decode, Encode};
use crate::merk::{BackingStore, ProofStore};
use crate::plugins::{PaidCall, PayableCall};
use crate::prelude::{sdk_compat, ConvertSdkTx, DefaultPlugins2, QueryPlugin, Shared, SignerCall};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;
use crate::store::{Read, Write};
use crate::{Error, Result};
use educe::Educe;
use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::Arc;

pub mod exec;
pub mod mock;
pub mod trace;
pub mod wallet;

pub use exec::Client;
pub use wallet::Wallet;

pub struct AppClient<T, C, S, W> {
    _pd: PhantomData<(T, S)>,
    client: C,
    wallet: W,
    store: Cell<Option<Store>>,
}

use crate::plugins::DefaultPlugins;

impl<T, C, S, W> AppClient<T, C, S, W>
where
    W: Clone,
{
    pub fn new(client: C, wallet: W) -> Self {
        Self {
            _pd: PhantomData,
            client,
            wallet,
            store: Cell::new(None),
        }
    }

    pub async fn call(
        &self,
        payer: impl FnOnce(&T) -> T::Call,
        payee: impl FnOnce(&T) -> T::Call,
    ) -> Result<()>
    where
        C: Client<DefaultPlugins<S, T>>,
        T: Call + State + Query + Default + Describe + ConvertSdkTx,
        W: Wallet,
        DefaultPlugins2<S, T>: Call + Query + State + Describe + Default,
        S: Symbol,
    {
        // let app = &T::default(); // TODO
        let chain_id = self
            .query_root(|app| Ok(app.inner.borrow().inner.inner.chain_id.to_vec()))
            .await?;
        let app = self.query(Ok).await?;

        let payer_call = payer(&app);
        let payer_call_bytes = payer_call.encode()?;
        let payer = <T as Call>::Call::decode(payer_call_bytes.as_slice())?;

        let paid = payee(&app);
        let call = PayableCall::Paid(PaidCall { payer, paid });
        let nonce = self.wallet.nonce_hint()?;
        // TODO: if nonce is none, query current value
        let call = crate::plugins::NonceCall {
            nonce,
            inner_call: call,
        };
        let call = [chain_id, call.encode()?].concat();
        let call = self.wallet.sign(&call)?;
        let call = sdk_compat::Call::Native(call);
        let call_bytes = call.encode()?;
        let call = Decode::decode(call_bytes.as_slice())?;
        self.client.call(&call).await?;

        Ok(())
    }

    pub async fn query_root<U, F: FnMut(DefaultPlugins<S, T>) -> Result<U>>(
        &self,
        mut op: F,
    ) -> Result<U>
    where
        DefaultPlugins2<S, T>: Query + Describe + State + Call,
        C: Client<DefaultPlugins<S, T>>,
        S: Symbol,
    {
        let store = self.store.take().unwrap_or(Store::with_partial_map_store());

        let (res, store) = exec::execute(store, &self.client, |app| op(app)).await?;

        self.store.replace(Some(store));

        Ok(res)
    }

    pub async fn query<U, F: FnMut(T) -> Result<U>>(&self, mut op: F) -> Result<U>
    where
        T: State + Query + Call + Describe + ConvertSdkTx,
        DefaultPlugins2<S, T>: Query + Describe + State + Call,
        C: Client<DefaultPlugins<S, T>>,
        S: Symbol,
    {
        let store = self.store.take().unwrap_or(Store::with_partial_map_store());

        let (res, store) = exec::execute(store, &self.client, |app| {
            op(app.inner.into_inner().inner.inner.inner.inner.inner.inner)
        })
        .await?;

        self.store.replace(Some(store));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    use crate::call::build_call;
    use crate::client::mock::MockClient;
    use crate::client::wallet::Unsigned;
    use crate::coins::Symbol;
    use crate::collections::{Deque, Map};
    use crate::orga;
    use crate::plugins::ConvertSdkTx;
    use crate::plugins::PaidCall;
    use crate::prelude::QueryPlugin;

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

    #[tokio::test]
    async fn plugin_client() -> Result<()> {
        type App = DefaultPlugins<Simp, Foo>;
        let mut store = Store::with_map_store();
        let mut app = App::default();

        {
            app.inner.borrow_mut().inner.inner.chain_id = b"foo".to_vec().try_into()?;
            let inner_app = &mut app.inner.borrow_mut().inner.inner.inner.inner.inner;

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
        }
        app.attach(store.clone())?;

        let mut bytes = vec![];
        app.flush(&mut bytes)?;
        store.put(vec![], bytes)?;

        let mut mock_client = MockClient::<App>::with_store(store);

        {
            let client = AppClient::<Foo, _, _, _>::new(&mut mock_client, Unsigned);

            let bar_b = client.query(|app| Ok(app.bar.b)).await?;
            assert_eq!(bar_b, 8);

            let value = client
                .query(|app| app.e.get(12)?.unwrap().get_from_map(14, 2))
                .await?;
            assert_eq!(value, Some(32));

            let value = client
                .query(|app| {
                    let x = app.e.get(12)?.unwrap();
                    let _ = app.e.get(13)?.unwrap();
                    x.get_from_map(14, 2)
                })
                .await?;
            assert_eq!(value, Some(32));

            let key = 13;
            let value = client
                .query(|app| Ok(app.deque.get(0)?.unwrap().get(key)?.unwrap().a))
                .await?;
            assert_eq!(value, 3);

            client
                .call(
                    |app| build_call!(app.bar.inc_b(4)),
                    |app| build_call!(app.my_method(1, 2, 3)),
                )
                .await?;
        }

        {
            let client = AppClient::<Foo, _, _, _>::new(&mut mock_client, Unsigned);
            let bar_b = client.query(|app| Ok(app.bar.b)).await?;
            assert_eq!(bar_b, 12);
        }

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
