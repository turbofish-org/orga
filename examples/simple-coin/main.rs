#![feature(trivial_bounds)]
#![feature(min_specialization)]
mod client;
mod coin;
mod staking;
use coin::SimpleCoin;

use orga::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .as_slice()
    {
        [_, "node"] => {
            Node::<SignerProvider<NonceProvider<SimpleCoin>>>::new("simple_coin")
                .reset()
                .run();
        }
        [_, "client"] => {
            println!("ran client");
        }
        _ => {}
    };
}
