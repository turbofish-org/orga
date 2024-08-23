#![feature(trivial_bounds)]

use orga::{
    call::Call,
    coins::{Accounts, Symbol},
    collections::Map,
    orga,
    plugins::{ConvertSdkTx, DefaultPlugins, PaidCall},
};

#[orga]
#[derive(Debug, Clone, Copy)]
pub struct FooCoin();

impl Symbol for FooCoin {
    const INDEX: u8 = 123;
    const NAME: &'static str = "FOO";
}

#[orga]
pub struct App {
    pub foo: u32,
    pub bar: u32,
    pub map: Map<u32, u32>,
    #[call]
    pub accounts: Accounts<FooCoin>,
}

#[tokio::main]
pub async fn main() {
    pretty_env_logger::init();

    let home = tempfile::TempDir::new().unwrap();
    let node = orga::abci::Node::<DefaultPlugins<FooCoin, App>>::new(
        home.path(),
        Some("orga-foo"),
        orga::abci::DefaultConfig {
            seeds: None,
            timeout_commit: None,
        },
    );
    node.await.run().await.unwrap();
    home.close().unwrap();
}

impl ConvertSdkTx for App {
    type Output = PaidCall<<App as Call>::Call>;

    fn convert(&self, _msg: &orga::plugins::sdk_compat::sdk::Tx) -> orga::Result<Self::Output> {
        todo!()
    }
}
