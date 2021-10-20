// use orga::client::Client;
use orga::client::Client;
use orga::coins::*;
use orga::encoding::{Decode, Encode};
use orga::plugins::load_keypair;
use orga::prelude::*;
use orga::{Error, Result};

#[derive(Encode, Decode)]
pub struct Simp;
impl Symbol for Simp {}

impl State for Simp {
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> Result<Self> {
        Ok(data)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

#[derive(State, Call, Client, Query)]
pub struct SimpleCoin {
    balances: Map<Address, Coin<Simp>>,
}

impl InitChain for SimpleCoin {
    fn init_chain(&mut self, _ctx: &InitChainCtx) -> Result<()> {
        let my_address = load_keypair().unwrap().public.to_bytes();
        println!("my address: {:?}", my_address);
        self.balances
            .insert(my_address.into(), Simp::mint(100).into())?;
        Ok(())
    }
}
impl BeginBlock for SimpleCoin {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        for entry in self.balances.iter()? {
            let (key, balance) = entry?;
            println!("{:?} has {}", *key, balance.amount.value);
        }
        println!("\n\n\n");
        // self.balances.insert()
        // self.balances.
        Ok(())
    }
}

impl SimpleCoin {
    #[call]
    pub fn transfer(&mut self, to: Address, amount: Amount<Simp>) -> Result<()> {
        let signer = self
            .context::<Signer>()
            .ok_or_else(|| Error::App("No signer context available".into()))?
            .signer
            .ok_or_else(|| Error::App("Transfer calls must be signed".into()))?;

        let mut sender = self.balances.entry(signer)?.or_default()?;
        let coins = sender.take(amount)?;
        let mut receiver = self.balances.entry(to)?.or_default()?;
        receiver.give(coins).unwrap();

        Ok(())
    }

    pub fn balances(&mut self) -> &mut Map<Address, Coin<Simp>> {
        &mut self.balances
    }
}
