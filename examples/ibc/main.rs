#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]
#![feature(fn_traits)]
#![feature(type_name_of_val)]
#![feature(generic_associated_types)]

use orga::ibc::{start_grpc, Ibc};
use orga::prelude::*;

#[derive(State, Query, Client, Call)]
pub struct Counter {
    count: u64,
    pub ibc: Ibc,
}

#[derive(State, Debug, Clone)]
pub struct Simp(());
impl Symbol for Simp {
    const INDEX: u8 = 1;
}

impl BeginBlock for Counter {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        if self.count % 50 == 0 {
            println!("count is {:?}", self.count);
        }
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
        dbg!(msg);
        todo!()
    }
}

impl AbciQuery for Counter {
    fn abci_query(&self, request: &messages::RequestQuery) -> Result<messages::ResponseQuery> {
        self.ibc.abci_query(request)
    }
}

type MyApp = DefaultPlugins<Simp, Counter, "ibc-example">;

fn app_client() -> TendermintClient<MyApp> {
    TendermintClient::new("http://localhost:26357").unwrap()
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
    let ibc_client = app_client().ibc.clone();
    start_grpc(ibc_client).await;
    std::thread::sleep(std::time::Duration::from_secs(1000));
}
