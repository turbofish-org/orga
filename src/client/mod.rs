use crate::call::Call;
use crate::describe::Describe;
use crate::encoding::{Decode, Encode};

use crate::abci::App;
use crate::migrate::MigrateInto;
use crate::plugins::{sdk_compat, ABCICall, ABCIPlugin, ConvertSdkTx};
use crate::plugins::{PaidCall, PayableCall};
use crate::query::Query;
use crate::state::State;
use crate::store::Store;

use crate::Result;

use std::cell::Cell;
use std::marker::PhantomData;

pub mod exec;
pub mod mock;
pub mod trace;
pub mod wallet;

pub use exec::Transport;
pub use wallet::Wallet;

pub trait Client<T: Query + Call> {
    fn query_sync<U, F: FnMut(T) -> Result<U>>(&self, f: F) -> Result<U>;

    fn call_sync(
        &self,
        payer: impl FnOnce(&T) -> T::Call,
        payee: impl FnOnce(&T) -> T::Call,
    ) -> Result<()>;
}

pub struct AppClient<T, U, Transport, Symbol, Wallet> {
    _pd: PhantomData<Symbol>,
    transport: Transport,
    wallet: Wallet,
    // TODO: don't keep store here, and let something in the Transport layer
    // handle persistence/joining/etc for composibility
    store: Cell<Option<Store>>,
    sub: fn(T) -> U,
}

pub mod sync {
    use super::*;

    pub use exec::sync::Transport;

    impl<T, U, Transport, Symbol, Wallet> Client<U> for AppClient<T, U, Transport, Symbol, Wallet>
    where
        Transport: exec::sync::Transport<ABCIPlugin<DefaultPlugins<Symbol, T>>>,
        T: App
            + Call
            + State
            + Query
            + Default
            + Describe
            + ConvertSdkTx<Output = PaidCall<T::Call>>
            + MigrateInto<T>,
        U: App
            + Call
            + State
            + Query
            + Default
            + Describe
            + ConvertSdkTx<Output = PaidCall<T::Call>>
            + MigrateInto<U>,
        Wallet: wallet::Wallet + Clone,
        Symbol: crate::coins::Symbol,
    {
        fn query_sync<R, F: FnMut(U) -> Result<R>>(&self, f: F) -> Result<R> {
            AppClient::query_sync(self, f)
        }

        fn call_sync(
            &self,
            _payer: impl FnOnce(&U) -> U::Call,
            _payee: impl FnOnce(&U) -> U::Call,
        ) -> Result<()> {
            todo!()
            // self.call(payer, payee)
        }
    }
}

use crate::plugins::DefaultPlugins;

impl<T, U, Transport, Symbol, Wallet> AppClient<T, U, Transport, Symbol, Wallet>
where
    Transport: exec::Transport<ABCIPlugin<DefaultPlugins<Symbol, T>>>,
    T: App
        + Call
        + State
        + Query
        + Default
        + Describe
        + ConvertSdkTx<Output = PaidCall<T::Call>>
        + MigrateInto<T>,
    Wallet: wallet::Wallet + Clone,
    Symbol: crate::coins::Symbol,
{
    pub fn new(client: Transport, wallet: Wallet) -> Self
    where
        T: Into<U>,
    {
        Self {
            _pd: PhantomData,
            transport: client,
            wallet,
            store: Cell::new(None),
            sub: Into::into,
        }
    }

    pub fn with_wallet<W2: wallet::Wallet>(
        self,
        wallet: W2,
    ) -> AppClient<T, U, Transport, Symbol, W2> {
        AppClient {
            _pd: PhantomData,
            transport: self.transport,
            wallet,
            store: self.store,
            sub: self.sub,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn sub<U2>(self, sub: fn(T) -> U2) -> AppClient<T, U2, Transport, Symbol, Wallet> {
        AppClient {
            _pd: PhantomData,
            transport: self.transport,
            wallet: self.wallet,
            store: self.store,
            sub,
        }
    }

    // TODO: support subclients
    // TODO: return object with result data (e.g. txid)
    pub async fn call(
        &self,
        payer: impl FnOnce(&U) -> T::Call,
        payee: impl FnOnce(&U) -> T::Call,
    ) -> Result<()> {
        let chain_id = self
            .query_root(|app| Ok(app.inner.inner.borrow().inner.inner.chain_id.to_vec()))
            .await?;
        let nonce = match self.wallet.address()? {
            None => None,
            Some(addr) => Some(
                self.query_root(|app| app.inner.inner.borrow_mut().inner.inner.inner.nonce(addr))
                    .await?
                    + 1,
            ),
        };
        let app = self.query(Ok).await?;

        let payer_call = payer(&app);
        let payer_call_bytes = payer_call.encode()?;
        let payer = <T as Call>::Call::decode(payer_call_bytes.as_slice())?;

        let paid = payee(&app);
        let call = PayableCall::Paid(PaidCall { payer, paid });
        let call = crate::plugins::NonceCall {
            nonce,
            inner_call: call,
        };
        let call = [chain_id, call.encode()?].concat();
        let call = self.wallet.sign(&call)?;
        let call = ABCICall::DeliverTx(sdk_compat::Call::Native(call));
        self.transport.call(call).await?;

        Ok(())
    }

    pub async fn query_root<U2, F2: FnMut(ABCIPlugin<DefaultPlugins<Symbol, T>>) -> Result<U2>>(
        &self,
        op: F2,
    ) -> Result<U2> {
        let store = self.store.take().unwrap_or_default();

        let (res, store) = exec::execute(store, &self.transport, op).await?;

        self.store.replace(Some(store));

        Ok(res)
    }

    pub async fn query<U2, F2: FnMut(U) -> Result<U2>>(&self, mut op: F2) -> Result<U2> {
        let store = self.store.take().unwrap_or_default();

        let (res, store) = exec::execute(store, &self.transport, |app| {
            let inner = app
                .inner
                .inner
                .into_inner()
                .inner
                .inner
                .inner
                .inner
                .inner
                .inner;
            op((self.sub)(inner))
        })
        .await?;

        self.store.replace(Some(store));

        Ok(res)
    }
}

impl<T, U, Transport, Symbol, Wallet> AppClient<T, U, Transport, Symbol, Wallet>
where
    Transport: exec::sync::Transport<ABCIPlugin<DefaultPlugins<Symbol, T>>>,
    T: App
        + Call
        + State
        + Query
        + Default
        + Describe
        + ConvertSdkTx<Output = PaidCall<T::Call>>
        + MigrateInto<T>,
    Wallet: wallet::Wallet + Clone,
    Symbol: crate::coins::Symbol,
{
    // TODO: support subclients
    // TODO: return object with result data (e.g. txid)
    pub fn call_sync(
        &self,
        payer: impl FnOnce(&U) -> T::Call,
        payee: impl FnOnce(&U) -> T::Call,
    ) -> Result<()> {
        let chain_id =
            self.query_root_sync(|app| Ok(app.inner.inner.borrow().inner.inner.chain_id.to_vec()))?;
        let nonce = match self.wallet.address()? {
            None => None,
            Some(addr) => Some(
                self.query_root_sync(|app| {
                    app.inner.inner.borrow_mut().inner.inner.inner.nonce(addr)
                })? + 1,
            ),
        };
        let app = self.query_sync(Ok)?;

        let payer_call = payer(&app);
        let payer_call_bytes = payer_call.encode()?;
        let payer = <T as Call>::Call::decode(payer_call_bytes.as_slice())?;

        let paid = payee(&app);
        let call = PayableCall::Paid(PaidCall { payer, paid });
        let call = crate::plugins::NonceCall {
            nonce,
            inner_call: call,
        };
        let call = [chain_id, call.encode()?].concat();
        let call = self.wallet.sign(&call)?;
        let call = ABCICall::DeliverTx(sdk_compat::Call::Native(call));
        exec::sync::Transport::call_sync(&self.transport, call)?;

        Ok(())
    }

    pub fn query_root_sync<U2, F2: FnMut(ABCIPlugin<DefaultPlugins<Symbol, T>>) -> Result<U2>>(
        &self,
        op: F2,
    ) -> Result<U2> {
        let store = self.store.take().unwrap_or_default();

        let (res, store) = exec::sync::execute(store, &self.transport, op)?;

        self.store.replace(Some(store));

        Ok(res)
    }

    pub fn query_sync<U2, F2: FnMut(U) -> Result<U2>>(&self, mut op: F2) -> Result<U2> {
        let store = self.store.take().unwrap_or_default();

        let (res, store) = exec::sync::execute(store, &self.transport, |app| {
            let inner = app
                .inner
                .inner
                .into_inner()
                .inner
                .inner
                .inner
                .inner
                .inner
                .inner;
            op((self.sub)(inner))
        })?;

        self.store.replace(Some(store));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::call::build_call;
    use crate::client::mock::MockClient;
    use crate::client::wallet::{DerivedKey, Unsigned};
    use crate::coins::{Address, Symbol};
    use crate::collections::{Deque, Map};
    use crate::context::Context;
    use crate::plugins::ConvertSdkTx;
    use crate::plugins::PaidCall;
    use crate::{orga, Error};
    use crate::{plugins::Signer, store::Write};

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

        fn convert(&self, _msg: &crate::plugins::sdk_compat::sdk::Tx) -> Result<Self::Output> {
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
        pub fn my_method(&mut self, _a: u32, _b: u8, _c: u16) -> Result<()> {
            self.b += 1;
            Ok(())
        }

        #[call]
        pub fn my_other_method(&mut self, _d: u32) -> Result<()> {
            self.c += 1;
            Ok(())
        }

        #[call]
        pub fn signed_method(&mut self, address: Address) -> Result<()> {
            let signer = Context::resolve::<Signer>().unwrap();
            if signer.signer != Some(address) {
                return Err(Error::App("wrong signer".into()));
            }

            self.my_field += 1;

            Ok(())
        }
    }

    type App = ABCIPlugin<DefaultPlugins<Simp, Foo>>;

    fn setup() -> Result<MockClient<App>> {
        let mut store = Store::with_map_store();
        let mut app = App::default();
        app.attach(store.clone())?;

        {
            app.inner.inner.borrow_mut().inner.inner.chain_id = b"foo".to_vec().try_into()?;

            let inner_app = &mut app
                .inner
                .inner
                .borrow_mut()
                .inner
                .inner
                .inner
                .inner
                .inner
                .inner;

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
        };

        let mut bytes = vec![];
        app.flush(&mut bytes)?;
        store.put(vec![], bytes)?;

        Ok(MockClient::<App>::with_store(store))
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn appclient() -> Result<()> {
        let mut mock_client = setup()?;

        {
            let client = AppClient::<Foo, Foo, _, _, _>::new(
                &mut mock_client,
                DerivedKey::new(b"alice").unwrap(),
            );

            let bar_b = client.query(|app| Ok(app.bar.b)).await?;
            assert_eq!(bar_b, 8);

            {
                // TODO: if bar doesn't drop, other queries will fail because
                // Bar holds on to a reference to the store. this should isntead
                // be locked at a different level so we can do concurrent
                // queries with the same client, and either join the values
                // separately or e.g. wait for equivalent queries to finish
                let bar = client.query(|app| Ok(app.bar)).await?;
                assert_eq!(bar.b, 8);
            }

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

            client.call(
                |app| build_call!(app.bar.inc_b(4)),
                |app| build_call!(app.signed_method(DerivedKey::address_for(b"alice").unwrap())),
            ).await?;
        }

        {
            let client = AppClient::<Foo, Foo, _, _, _>::new(&mut mock_client, Unsigned);
            let bar_b = client.query(|app| Ok(app.bar.b)).await?;
            assert_eq!(bar_b, 12);
        }

        Ok(())
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn sub() -> Result<()> {
        let mut mock_client = setup()?;
        let client = AppClient::<Foo, Foo, _, _, _>::new(
            &mut mock_client,
            DerivedKey::new(b"alice").unwrap(),
        );

        let bar_client = client.sub(|app| app.bar);

        let bar_b = bar_client.query(|bar| Ok(bar.b)).await?;
        assert_eq!(bar_b, 8);

        // TODO
        // bar_client
        //     .call(
        //         |bar| build_call!(bar.inc_b(4)),
        //         |bar| build_call!(bar.inc_b(4)),
        //     )
        //     .await?;

        Ok(())
    }

    #[serial_test::serial]
    #[test]
    fn appclient_sync() -> Result<()> {
        let mut mock_client = setup()?;

        {
            let client = AppClient::<Foo, Foo, _, _, _>::new(
                &mut mock_client,
                DerivedKey::new(b"alice").unwrap(),
            );

            let bar_b = client.query_sync(|app| Ok(app.bar.b))?;
            assert_eq!(bar_b, 8);

            {
                // TODO: if bar doesn't drop, other queries will fail because
                // Bar holds on to a reference to the store. this should isntead
                // be locked at a different level so we can do concurrent
                // queries with the same client, and either join the values
                // separately or e.g. wait for equivalent queries to finish
                let bar = client.query_sync(|app| Ok(app.bar))?;
                assert_eq!(bar.b, 8);
            }

            let value = client.query_sync(|app| app.e.get(12)?.unwrap().get_from_map(14, 2))?;
            assert_eq!(value, Some(32));

            let value = client.query_sync(|app| {
                let x = app.e.get(12)?.unwrap();
                let _ = app.e.get(13)?.unwrap();
                x.get_from_map(14, 2)
            })?;
            assert_eq!(value, Some(32));

            let key = 13;
            let value =
                client.query_sync(|app| Ok(app.deque.get(0)?.unwrap().get(key)?.unwrap().a))?;
            assert_eq!(value, 3);

            client.call_sync(
                |app| build_call!(app.bar.inc_b(4)),
                |app| build_call!(app.signed_method(DerivedKey::address_for(b"alice").unwrap())),
            )?;
        }

        {
            let client = AppClient::<Foo, Foo, _, _, _>::new(&mut mock_client, Unsigned);
            let bar_b = client.query_sync(|app| Ok(app.bar.b))?;
            assert_eq!(bar_b, 12);
        }

        Ok(())
    }

    #[serial_test::serial]
    #[test]
    fn sub_sync() -> Result<()> {
        let mut mock_client = setup()?;
        let client = AppClient::<Foo, Foo, _, _, _>::new(
            &mut mock_client,
            DerivedKey::new(b"alice").unwrap(),
        );

        let bar_client = client.sub(|app| app.bar);

        let bar_b = bar_client.query_sync(|bar| Ok(bar.b))?;
        assert_eq!(bar_b, 8);

        // TODO
        // bar_client
        //     .call(
        //         |bar| build_call!(bar.inc_b(4)),
        //         |bar| build_call!(bar.inc_b(4)),
        //     )
        //     .await?;

        Ok(())
    }

    #[test]
    #[serial_test::serial]
    fn generic_sync() {
        let mut mock_client = setup().unwrap();
        let client = AppClient::<Foo, Foo, _, _, _>::new(
            &mut mock_client,
            DerivedKey::new(b"alice").unwrap(),
        );

        fn do_query(client: impl Client<Foo>) {
            let bar_b = client.query_sync(|app| Ok(app.bar.b)).unwrap();
            assert_eq!(bar_b, 8);
        }

        do_query(client);
    }
}
