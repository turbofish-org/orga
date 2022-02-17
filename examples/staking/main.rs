#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]

use orga::prelude::*;
use rust_decimal_macros::dec;

#[derive(State, Debug, Clone)]
pub struct MyCoin(());
impl Symbol for MyCoin {}

#[derive(State, Call, Query, Client)]
pub struct StakingApp {
    pub accounts: Accounts<MyCoin>,
    pub staking: Staking<MyCoin>,
}

impl BeginBlock for StakingApp {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.staking.begin_block(ctx)?;
        Ok(())
    }
}

impl EndBlock for StakingApp {
    fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
        self.staking.end_block(ctx)?;
        Ok(())
    }
}
impl InitChain for StakingApp {
    fn init_chain(&mut self, _ctx: &InitChainCtx) -> Result<()> {
        self.accounts.deposit(my_address(), 100_000_000.into())?;
        self.accounts.allow_transfers(true);
        self.staking.set_min_self_delegation(100);

        Ok(())
    }
}

type MyApp = DefaultPlugins<MyCoin, StakingApp, "staking-example">;

fn rpc_client() -> TendermintClient<MyApp> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

fn my_address() -> Address {
    Address::from_pubkey(load_keypair().unwrap().public.serialize())
}

async fn my_balance() -> Result<Amount> {
    let address = my_address();
    let client = rpc_client();
    type AppQuery = <StakingApp as Query>::Query;
    type AcctQuery = <Accounts<MyCoin> as Query>::Query;

    let q = NonceQuery::Inner(AppQuery::FieldAccounts(AcctQuery::MethodBalance(
        address,
        vec![],
    )));
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

    sleep(Duration::from_secs(30));
    let bal = my_balance().await.unwrap();
    println!("My balance: {:?}", bal);

    rpc_client()
        .accounts
        .transfer(Address::from_pubkey([0; 33]), 100.into())
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

    let my_tm_key = [
        201, 225, 191, 2, 35, 17, 176, 124, 63, 174, 96, 139, 146, 170, 57, 162, 84, 58, 108, 78,
        93, 173, 77, 235, 53, 183, 132, 146, 213, 150, 196, 144,
    ];
    rpc_client()
        .pay_from(async move |mut client| client.accounts.take_as_funding(350.into()).await)
        .staking
        .declare_self(
            my_tm_key,
            dec!(0.0).into(),
            dec!(1.0).into(),
            dec!(0.1).into(),
            0.into(),
            350.into(),
            vec![].into(),
        )
        .await
        .unwrap();

    rpc_client()
        .pay_from(async move |mut client| client.accounts.take_as_funding(250.into()).await)
        .staking
        .delegate_from_self(my_address(), 250.into())
        .await
        .unwrap();

    rpc_client()
        .staking
        .unbond_self(my_address(), 100.into())
        .await
        .unwrap();

    rpc_client()
        .pay_from(async move |mut client| client.accounts.take_as_funding(100.into()).await)
        .staking
        .delegate_from_self(Address::from_pubkey([0; 33]), 100.into())
        .await
        .unwrap_or_else(|e| {
            println!("{:?}", e);
        });

    let bal = my_balance().await.unwrap();
    println!("My balance: {:?}", bal);

    handle.join().unwrap();
}
