#![feature(trivial_bounds)]
#![feature(min_specialization)]

mod counter;
mod multicounter;

use counter::*;
use multicounter::*;
use orga::prelude::*;
use tokio::task::spawn_blocking;

pub type CounterApp = SignerPlugin<NoncePlugin<MultiCounter>>;

type CounterQuery = counter_query::Query;
type MultiCounterQuery = multi_counter_query::Query;
type MapQuery = <Map<Address, Counter> as Query>::Query;

pub async fn run_client() -> Result<()> {
    let mut client = TendermintClient::<CounterApp>::new("http://localhost:26657")?;

    let my_address = Address::from_pubkey([
        194, 42, 183, 160, 59, 68, 203, 90, 200, 61, 123, 126, 110, 150, 217, 245, 196, 90, 179,
        178, 179, 193, 107, 118, 13, 117, 195, 236, 191, 213, 145, 148, 120,
    ]);

    let query_my_count = || {
        let count = CounterQuery::FieldCount(()).encode().unwrap();
        let map_get = MapQuery::MethodGet(my_address, count);
        MultiCounterQuery::FieldCounters(map_get)
    };

    println!(
        "count before incrementing: {:?}",
        client
            .query(query_my_count(), |state| Ok(state
                .counters
                .get(my_address)?
                .map(|c| c.count)),)
            .await?,
    );

    client.increment().await?;

    println!(
        "count after incrementing: {:?}",
        client
            .query(query_my_count(), |state| Ok(state
                .counters
                .get(my_address)?
                .map(|c| c.count)),)
            .await?,
    );

    Ok(())
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
