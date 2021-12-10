#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]

use orga::prelude::*;

#[derive(State, Debug, Clone)]
pub struct MyCoin(());
impl Symbol for MyCoin {}

#[derive(State, Call, Query, Client)]
pub struct StakingApp {
    pub accounts: Accounts<MyCoin>,
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
