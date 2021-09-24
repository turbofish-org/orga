#![feature(trivial_bounds)]
#![feature(min_specialization)]
mod client;
mod coin;
mod staking;
use coin::{Simp, SimpleCoin};

use orga::prelude::*;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .as_slice()
    {
        [_, "node"] => {
            tokio::task::spawn_blocking(|| {
                Node::<SignerProvider<NonceProvider<SimpleCoin>>>::new("simple_coin")
                    .reset()
                    .run()
            })
            .await
            .unwrap();
        }
        [_, "client"] => {
            // type WholeApp = SignerProvider<NonceProvider<SimpleCoin>>;
            // let client: TendermintClient<WholeApp> =
            //     TendermintClient::new("http://localhost:26657").unwrap();
            // let mut client = WholeApp::create_client(client);
            // // let my_address = load_keypair().unwrap().public.to_bytes();
            // client.transfer([123; 32], 5.into()).await.unwrap();
        }
        _ => {
            println!("hit catchall")
        }
    };
}
