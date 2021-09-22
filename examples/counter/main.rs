#![feature(trivial_bounds)]
#![feature(min_specialization)]

mod client;
mod counter;
mod multicounter;
mod node;

use client::run_client;
use counter::*;
use multicounter::*;
use node::run_node;
use orga::prelude::*;

pub type CounterApp = SignerProvider<NonceProvider<MultiCounter>>;

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
            tokio::task::spawn_blocking(run_node).await;
        }
        [_, "client"] => {
            run_client().await.unwrap();
        }
        _ => {}
    };
}
