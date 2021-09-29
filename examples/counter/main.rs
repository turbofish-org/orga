#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(generic_associated_types)]

mod counter;
mod multicounter;

use counter::*;
use multicounter::*;
use orga::prelude::*;
use tokio::task::spawn_blocking;

pub type CounterApp = SignerProvider<NonceProvider<MultiCounter>>;

pub async fn run_client() -> Result<()> {
    let mut client = TendermintClient::<CounterApp>::new("http://localhost:26657")?;

    client.increment().await
}

pub fn run_node() {
    Node::<CounterApp>::new("my_counter").reset().run();
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "node" => spawn_blocking(run_node).await.unwrap(),
        "client" => run_client().await.unwrap(),

        _ => {}
    };
}
