#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]
#![feature(fn_traits)]
#![feature(type_name_of_val)]

use orga::ibc::{start_grpc, Ibc};
use orga::prelude::*;

#[derive(State, Query, Client, Call)]
pub struct Counter {
    count: u64,
}

#[derive(State, Debug, Clone)]
pub struct Simp(());
impl Symbol for Simp {}

impl BeginBlock for Counter {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        println!("count is {:?}", self.count);
        self.count += 1;

        Ok(())
    }
}

impl EndBlock for Counter {
    fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
        Ok(())
    }
}

impl InitChain for Counter {
    fn init_chain(&mut self, ctx: &InitChainCtx) -> Result<()> {
        Ok(())
    }
}

impl ConvertSdkTx for Counter {
    type Output = PaidCall<<Counter as Call>::Call>;
    fn convert(&self, msg: &sdk_compat::sdk::Tx) -> Result<Self::Output> {
        todo!()
    }
}

type MyApp = DefaultPlugins<Simp, Counter, "ibc-example">;

fn app_client() -> TendermintClient<MyApp> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

#[tokio::main]
async fn main() {
    println!("Running IBC example");
    std::thread::spawn(|| {
        Node::<MyApp>::new("ibc-example", Default::default())
            .reset()
            .run()
            .unwrap();
    });
    std::thread::sleep(std::time::Duration::from_secs(4));
    let ibc_client = app_client().ibc();
    start_grpc(ibc_client).await;
    std::thread::sleep(std::time::Duration::from_secs(1000));
}
