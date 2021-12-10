#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]

use orga::prelude::*;

#[derive(State, Debug, Clone)]
pub struct MyCoin(());
impl Symbol for MyCoin {}

#[derive(State, Call, Query)]
pub struct StakingApp {
    pub accounts: Accounts<MyCoin>,
}

pub mod staking_app_client {
    use super::*;
    #[must_use]
    pub struct Client<__Parent>
    where
        __Parent: Clone + Send,
    {
        pub(super) parent: __Parent,
        pub accounts:
            <Accounts<MyCoin> as ::orga::client::Client<FieldAccountsAdapter<__Parent>>>::Client,
    }
    impl<__Parent> Clone for Client<__Parent>
    where
        __Parent: Clone + Send,
    {
        fn clone(&self) -> Self {
            Self {
                parent: self.parent.clone(),
                accounts: self.accounts.clone(),
            }
        }
    }
    impl<__Parent> Client<__Parent>
    where
        __Parent: Clone + Send,
    {
        pub fn new(parent: __Parent) -> Self {
            use ::orga::client::Client as _;
            Client {
                accounts: Accounts::<MyCoin>::create_client(FieldAccountsAdapter::new(
                    parent.clone(),
                )),
                parent,
            }
        }
    }
    pub struct FieldAccountsAdapter<__Parent>
    where
        __Parent: Clone + Send,
    {
        pub(super) parent: __Parent,
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl<__Parent: ::core::clone::Clone> ::core::clone::Clone for FieldAccountsAdapter<__Parent>
    where
        __Parent: Clone + Send,
    {
        #[inline]
        fn clone(&self) -> FieldAccountsAdapter<__Parent> {
            match *self {
                FieldAccountsAdapter {
                    parent: ref __self_0_0,
                } => FieldAccountsAdapter {
                    parent: ::core::clone::Clone::clone(&(*__self_0_0)),
                },
            }
        }
    }
    impl<__Parent> FieldAccountsAdapter<__Parent>
    where
        __Parent: Clone + Send,
    {
        pub fn new(parent: __Parent) -> Self {
            Self { parent }
        }
    }
    impl<__Parent> ::orga::client::AsyncCall for FieldAccountsAdapter<__Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <StakingApp as ::orga::call::Call>::Call>,
    {
        type Call = <Accounts<MyCoin> as ::orga::call::Call>::Call;
        #[allow(
            clippy::let_unit_value,
            clippy::type_complexity,
            clippy::type_repetition_in_bounds,
            clippy::used_underscore_binding
        )]
        fn call<'life0, 'async_trait>(
            &'life0 mut self,
            call: Self::Call,
        ) -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = Result<()>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                if let ::core::option::Option::Some(__ret) =
                    ::core::option::Option::None::<Result<()>>
                {
                    return __ret;
                }
                let mut __self = self;
                let call = call;
                let __ret: Result<()> = {
                    let subcall_bytes = ::orga::encoding::Encode::encode(&call)?;
                    let subcall =
                        <StakingApp as ::orga::call::Call>::Call::FieldAccounts(subcall_bytes);
                    __self.parent.call(subcall).await
                };
                #[allow(unreachable_code)]
                __ret
            })
        }
    }
}
impl<__Parent> ::orga::client::Client<__Parent> for StakingApp
where
    __Parent: Clone + Send,
{
    type Client = staking_app_client::Client<__Parent>;
    fn create_client(parent: __Parent) -> Self::Client {
        staking_app_client::Client::new(parent)
    }
}

impl InitChain for StakingApp {
    fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
        self.accounts.deposit(my_address(), 100_000_000.into())?;

        Ok(())
    }
}

type MyApp = DefaultPlugins<StakingApp>;

fn rpc_client() -> TendermintClient<MyApp> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

fn my_address() -> Address {
    load_keypair().unwrap().public.to_bytes().into()
}

async fn my_balance() -> Result<Amount> {
    let address = my_address();
    let client = rpc_client();
    type AppQuery = <MyApp as Query>::Query;
    type AcctQuery = <Accounts<MyCoin> as Query>::Query;

    let q = AppQuery::FieldAccounts(AcctQuery::MethodBalance(address, vec![]));
    let balance = client
        .query(q, |state| state.accounts.balance(address))
        .await?;

    Ok(balance)
}

#[tokio::main]
async fn main() {
    use std::thread::{sleep, spawn};
    use std::time::Duration;

    let handle = spawn(|| {
        println!("Running node");
        Node::<MyApp>::new("staking_app").reset().run()
    });

    sleep(Duration::from_secs(1));
    let bal = my_balance().await.unwrap();
    println!("My balance: {:?}", bal);

    // fn print_name<T>(t: &T) {
    //     dbg!(std::any::type_name::<T>());
    // }
    // print_name(&rpc_client().accounts);

    rpc_client()
        .accounts
        .transfer([0; 32].into(), 100.into())
        .await
        .unwrap();
    println!("Sent coins");
    let bal = my_balance().await.unwrap();
    println!("My balance: {:?}", bal);

    rpc_client()
        .pay_from(async move |mut client| client.accounts.take_as_funding(123.into()).await)
        .accounts
        .give_from_funding(122.into())
        .await
        .unwrap();

    let bal = my_balance().await.unwrap();
    println!("My balance: {:?}", bal);

    handle.join().unwrap();
}
