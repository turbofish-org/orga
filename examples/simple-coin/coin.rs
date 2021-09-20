use orga::coins::*;
use orga::encoding::{Decode, Encode, Terminated};
use orga::prelude::*;

#[derive(Encode, Decode)]
pub struct Simp;
impl Symbol for Simp {}

#[derive(State)]
pub struct SimpleCoin {
    pub balances: Map<Address, Coin<Simp>>,
}

impl SimpleCoin {
    #[call]
    pub fn transfer(&mut self, to: Address, amount: Amount<Simp>) -> Result<()> {
        let signer = self
            .context::<Signer>()
            .ok_or_else(|| failure::format_err!("No signer context available"))?
            .signer
            .ok_or_else(|| failure::format_err!("Transfer calls must be signed"))?;

        let mut sender = self.balances.entry(signer)?.or_default()?;
        let coins = sender.take(amount)?;
        let mut receiver = self.balances.entry(to)?.or_default()?;
        receiver.give(coins);

        Ok(())
    }

    pub fn balances(&mut self) -> &mut Map<Address, Coin<Simp>> {
        &mut self.balances
    }
}