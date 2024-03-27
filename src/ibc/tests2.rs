use std::path::Path;
use std::time::Duration;

use async_process::{Child, Command, Stdio};
use ibc::applications::transfer::context::TokenTransferExecutionContext;
use orga::abci::{BeginBlock, Node};
use orga::call::build_call;
use orga::client::wallet::{DerivedKey, Unsigned};
use orga::client::AppClient;
use orga::coins::{Accounts, Address, Amount, Coin, Symbol};
use orga::context::{Context, GetContext};
use orga::ibc::transfer::Denom;
use orga::ibc::{start_grpc, GrpcOpts, Ibc, IbcTx, RawIbcTx};
use orga::orga;
use orga::plugins::MIN_FEE;
use orga::plugins::{
    disable_fee, sdk_compat::sdk::Tx as SdkTx, ChainId, ConvertSdkTx, DefaultPlugins, Paid,
    PaidCall, Signer,
};
use orga::prelude::*;
use orga::tendermint::client::HttpClient;
use tempdir::TempDir;

use crate::abci::AbciQuery;

#[orga]
#[derive(Debug, Clone, Copy)]
pub struct FooCoin();

impl Symbol for FooCoin {
    const INDEX: u8 = 123;
}

#[orga]
pub struct IbcApp {
    pub a: u32,
    pub b: u32,
    #[call]
    pub ibc: Ibc,
    #[call]
    pub accounts: Accounts<FooCoin>,
}

#[orga]
impl IbcApp {
    #[call]
    pub fn inc_a(&mut self) -> Result<()> {
        disable_fee();
        self.a += 1;
        Ok(())
    }

    #[call]
    pub fn inc_b(&mut self) -> Result<()> {
        disable_fee();
        self.b += 1;
        Ok(())
    }

    #[call]
    pub fn mint(&mut self, amount: Amount) -> Result<()> {
        disable_fee();
        let ctx = self.context::<Paid>().unwrap();
        ctx.give::<FooCoin, _>(amount)
    }

    #[call]
    pub fn ibc_deposit_foo(&mut self, to: Address, amount: Amount) -> Result<()> {
        disable_fee();
        let signer = self.signer()?;
        let coins = self.accounts.withdraw(signer, amount)?;

        self.ibc.mint_coins_execute(&to, &coins.into())?;

        Ok(())
    }

    #[call]
    pub fn ibc_withdraw_foo(&mut self, amount: Amount) -> Result<()> {
        disable_fee();
        let signer = self.signer()?;

        let coins: Coin<FooCoin> = amount.into();
        self.ibc.burn_coins_execute(&signer, &coins.into())?;
        self.accounts.deposit(signer, amount.into())
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Signer("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Unauthorized account action".into()))
    }
}

impl BeginBlock for IbcApp {
    fn begin_block(&mut self, ctx: &orga::plugins::BeginBlockCtx) -> Result<()> {
        self.ibc.begin_block(ctx)?;
        Ok(())
    }
}

impl ConvertSdkTx for IbcApp {
    type Output = PaidCall<<IbcApp as Call>::Call>;

    fn convert(&self, sdk_tx: &SdkTx) -> orga::Result<Self::Output> {
        match sdk_tx {
            SdkTx::Protobuf(tx) => {
                let ibc_tx_res = IbcTx::try_from(tx.clone());
                if ibc_tx_res.is_ok() {
                    let ibc_tx = RawIbcTx(tx.clone());
                    let paid = build_call!(self.ibc.deliver(ibc_tx.clone()));
                    return Ok(PaidCall {
                        payer: build_call!(self.mint(MIN_FEE.into())),
                        paid,
                    });
                }

                todo!()
            }
            _ => todo!(),
        }
    }
}

pub fn spawn_node() {
    std::thread::spawn(move || {
        Context::add(ChainId("orga-ibc-test".to_string()));

        let home = TempDir::new("ibc-test-node").unwrap();
        let node: Node<DefaultPlugins<FooCoin, IbcApp>> = Node::new(
            home.path(),
            "orga-ibc-test",
            orga::abci::DefaultConfig {
                seeds: None,
                timeout_commit: None,
            },
        )
        .tendermint_flags(vec![
            "--rpc.laddr".to_string(),
            "tcp://0.0.0.0:26667".to_string(),
            "--p2p.laddr".to_string(),
            "tcp://0.0.0.0:26666".to_string(),
        ]);

        node.run().unwrap();
        home.close().unwrap();
    });
}

async fn start_grpc_server() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            start_grpc(
                || client().sub(|app| app.ibc),
                &GrpcOpts {
                    host: "127.0.0.1".to_string(),
                    port: 9001,
                },
            )
            .await
        })
        .await
}

fn client() -> AppClient<IbcApp, IbcApp, HttpClient, FooCoin, DerivedKey> {
    let client = HttpClient::new("http://localhost:26667").unwrap();
    AppClient::<IbcApp, IbcApp, _, FooCoin, _>::new(client, DerivedKey::new(b"alice").unwrap())
}

async fn spawn_gaia(home: &Path) -> Result<Child> {
    let node_home = home.join("node0").join("gaiad");
    println!("initializing gaiad testnet at {:?}", home);
    Command::new("gaiad")
        .arg("testnet")
        .arg("-o")
        .arg(home)
        .arg("--v")
        .arg("1")
        .arg("--keyring-backend")
        .arg("test")
        .arg("--minimum-gas-prices")
        .arg("0.0stake")
        .arg("--chain-id")
        .arg("ibc-0")
        .spawn()?
        .status()
        .await?;

    let cmd = Command::new("gaiad")
        .arg("start")
        .arg("--home")
        .arg(node_home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(cmd)
}

async fn gaia_address(home: &Path) -> Result<String> {
    let node_home = home.join("node0").join("gaiad");
    let out = Command::new("gaiad")
        .args(["--home", node_home.to_str().unwrap()])
        .args(["--keyring-backend", "test"])
        .args(["keys", "list"])
        .output()
        .await
        .unwrap();

    let address = String::from_utf8(out.stdout)
        .unwrap()
        .split_once("address: ")
        .unwrap()
        .1
        .split_once('\n')
        .unwrap()
        .0
        .to_string();

    Ok(address)
}

async fn gaia_send(home: &Path, from: &str, to: &str, coin: &str) -> Result<()> {
    let node_home = home.join("node0").join("gaiad");
    let out = Command::new("gaiad")
        .args(["--home", node_home.to_str().unwrap()])
        .args(["--keyring-backend", "test"])
        .args(["--chain-id", "ibc-0"])
        .args(["tx", "bank", "send", from, to, coin, "--yes"])
        .output()
        .await
        .unwrap();

    Ok(())
}

impl AbciQuery for IbcApp {
    fn abci_query(
        &self,
        request: &tendermint_proto::v0_34::abci::RequestQuery,
    ) -> Result<tendermint_proto::v0_34::abci::ResponseQuery> {
        self.ibc.abci_query(request)
    }
}

#[ignore]
#[tokio::test]
#[serial_test::serial]
async fn ibc_app() -> Result<()> {
    pretty_env_logger::init();
    spawn_node();
    let gaia_home = TempDir::new("ibc-0").unwrap();
    let gaia_path = gaia_home.path().to_path_buf();
    tokio::spawn(async move {
        let gaia_path = gaia_path.as_path();
        spawn_gaia(gaia_path).await.unwrap()
    });

    tokio::time::sleep(Duration::from_secs(5)).await;
    let gaia_path = gaia_home.path().to_path_buf();
    let op_addr = gaia_address(gaia_path.as_path()).await?;
    dbg!(&op_addr);
    const GAIA_RELAYER_ADDR: &str = "cosmos1rk07saqmvfle50h4h9hul00g67xzrcc5cn6up3";
    tokio::time::sleep(Duration::from_secs(5)).await;
    gaia_send(
        gaia_path.as_path(),
        &op_addr,
        GAIA_RELAYER_ADDR,
        "1000000stake",
    )
    .await
    .unwrap();

    // let gaiad = spawn_gaia(gaia_home.path()).await?;
    // println!("gaiad output: {:?}", gaiad.output().await?);

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            // tokio::task::spawn_local(async move {
            start_grpc(
                || client().sub(|app| app.ibc),
                &GrpcOpts {
                    host: "127.0.0.1".to_string(),
                    port: 9001,
                },
            )
            .await;
            // });
        })
        .await;
    tokio::time::sleep(Duration::from_secs(1500)).await;
    let alice_address = DerivedKey::new(b"alice").unwrap().address();
    let foo_denom: Denom = FooCoin::INDEX.to_string().try_into().unwrap();

    let res = client().query(|app| Ok(app.a)).await?;
    assert_eq!(res, 0);
    client()
        .call(
            |app| build_call!(app.inc_a()),
            |app| build_call!(app.inc_b()),
        )
        .await
        .unwrap();

    // let res = client().query(|app| Ok(app.a)).await?;
    // assert_eq!(res, 1);

    // let res = client()
    //     .query(|app| app.ibc.transfer.balance(alice_address, foo_denom.clone()))
    //     .await?;
    // assert_eq!(res, 0);

    // // mint 1m foo to alice
    // client()
    //     .call(
    //         |app| build_call!(app.mint(1_000_000.into())),
    //         |app| build_call!(app.accounts.give_from_funding_all()),
    //     )
    //     .await?;

    // // move foo to ibc escrow
    // client()
    //     .call(
    //         |app| build_call!(app.inc_a()),
    //         |app| build_call!(app.ibc_deposit_foo(alice_address, 1_000_000.into())),
    //     )
    //     .await?;

    // let res = client()
    //     .query(|app| app.ibc.transfer.balance(alice_address, foo_denom.clone()))
    //     .await?;
    // assert_eq!(res, 1_000_000);

    drop(gaia_home);

    Ok(())
}
