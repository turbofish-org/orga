#![feature(min_specialization)]
use orga::coins::*;
use orga::prelude::*;

#[derive(Clone, Default, Copy)]
struct BTC;
impl Symbol for BTC {}

#[derive(State, Call, Query)]
pub struct SimpleCoin {
    btc: Map<[u8; 32], Coin<BTC>>,
}

impl SimpleCoin {
    pub fn transfer(&mut self, to: [u8; 32], amount: Amount<BTC>) -> Result<()> {
        let signer = self
            .context::<Signer>()
            .ok_or_else(|| failure::format_err!("No signer context available"))?;

        let sender = self.btc.entry(&signer.signer)?.or_default()?;
        let receiver = self.btc.entry(&to)?.or_default()?;

        Ok(())
    }
}

fn main() {
    Node::<SimpleCoin>::new("simp_coin").reset().run();
}
