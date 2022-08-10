#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]
#![feature(fn_traits)]
#![feature(type_name_of_val)]
#![feature(generic_associated_types)]

use orga::abci::tendermint_client::TendermintAdapter;
use orga::ibc::{start_grpc, Ibc, IbcTx};
use orga::prelude::*;

#[derive(State, Query, Client, Call)]
pub struct Counter {
    count: u64,
    pub ibc: Ibc,
}

impl Counter {
    #[call]
    pub fn noop(&mut self) -> Result<()> {
        Ok(())
    }

    #[call]
    pub fn mint_simp(&mut self, amount: Amount) -> Result<()> {
        let signer = self.signer()?;
        self.ibc.bank_mut().mint(signer, amount, "simp".parse()?)
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| orga::Error::Signer("No Signer context available".into()))?
            .signer
            .ok_or_else(|| orga::Error::Coins("Unauthorized account action".into()))
    }
}

#[derive(State, Debug, Clone)]
pub struct Simp(());

impl Symbol for Simp {
    const INDEX: u8 = 1;
}

impl BeginBlock for Counter {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        if self.count % 50 == 0 {
            println!("count is {:?}", self.count);
        }
        self.count += 1;

        self.ibc.begin_block(ctx)
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
        type AppCall = <Counter as Call>::Call;

        let tx_bytes = msg.encode()?;
        let _ibc_tx = IbcTx::decode(tx_bytes.as_slice())?;
        let deliver_msg_call_bytes = [vec![5], tx_bytes].concat();

        let paid_call = AppCall::FieldIbc(deliver_msg_call_bytes);
        Ok(PaidCall {
            payer: AppCall::MethodNoop(vec![]),
            paid: paid_call,
        })
    }
}

impl AbciQuery for Counter {
    fn abci_query(&self, request: &messages::RequestQuery) -> Result<messages::ResponseQuery> {
        self.ibc.abci_query(request)
    }
}

type MyApp = DefaultPlugins<Simp, Counter, "orga-0">;

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
    let app_client = app_client();
    std::thread::sleep(std::time::Duration::from_secs(4));
    start_grpc(
        app_client.clone(),
        app_client.ibc.clone(),
        &|client| client.ibc.clone(),
        9001,
    )
    .await;
    std::thread::sleep(std::time::Duration::from_secs(1000));
}
