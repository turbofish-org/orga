#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(async_closure)]
#![feature(fn_traits)]
#![feature(type_name_of_val)]

use orga::ibc::{GetIbcClient, Ibc};
use orga::prelude::*;

#[derive(State, Query, Client, Call)]
pub struct Counter {
    count: u64,
    pub ibc: Ibc,
}

impl BeginBlock for Counter {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        println!("count is {:?}", self.count);
        self.count += 1;

        Ok(())
    }
}

type MyApp = DefaultPlugins<Counter>;

fn app_client() -> TendermintClient<MyApp> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

use orga::ibc::run_relayer;

fn relay() {
    println!("calling into relayer..");
    std::thread::sleep(std::time::Duration::from_secs(2));

    run_relayer::<Relayer>();
}

type IbcClientType = orga::ibc::ibc_client::Client<
    self::counter_client::FieldIbcAdapter<
        orga::plugins::payable::UnpaidAdapter<
            self::Counter,
            orga::plugins::nonce::NonceClient<
                orga::plugins::payable::PayablePlugin<self::Counter>,
                orga::plugins::signer::SignerClient<
                    orga::plugins::nonce::NoncePlugin<
                        orga::plugins::payable::PayablePlugin<self::Counter>,
                    >,
                    orga::abci::tendermint_client::TendermintAdapter<
                        orga::plugins::signer::SignerPlugin<
                            orga::plugins::nonce::NoncePlugin<
                                orga::plugins::payable::PayablePlugin<self::Counter>,
                            >,
                        >,
                    >,
                >,
            >,
        >,
    >,
>;

type IbcClientParent = self::counter_client::FieldIbcAdapter<
    orga::plugins::payable::UnpaidAdapter<
        self::Counter,
        orga::plugins::nonce::NonceClient<
            orga::plugins::payable::PayablePlugin<self::Counter>,
            orga::plugins::signer::SignerClient<
                orga::plugins::nonce::NoncePlugin<
                    orga::plugins::payable::PayablePlugin<self::Counter>,
                >,
                orga::abci::tendermint_client::TendermintAdapter<
                    orga::plugins::signer::SignerPlugin<
                        orga::plugins::nonce::NoncePlugin<
                            orga::plugins::payable::PayablePlugin<self::Counter>,
                        >,
                    >,
                >,
            >,
        >,
    >,
>;
struct Relayer;

impl GetIbcClient for Relayer {
    type Parent = IbcClientParent;
    fn get_ibc_client() -> IbcClientType {
        app_client().ibc.clone()
    }
}

fn main() {
    println!("Running IBC example");
    std::thread::spawn(|| {
        Node::<MyApp>::new(".mycounter").reset().run();
    });

    std::thread::spawn(|| {
        Node::<MyApp>::new(".mycounter-2")
            .reset()
            .p2p_port(26666)
            .rpc_port(26667)
            .abci_port(26668)
            .run();
    });

    relay();

    std::thread::sleep(std::time::Duration::from_secs(100));
}
