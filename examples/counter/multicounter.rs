use super::Counter;
use orga::{coins::Address, prelude::*};

#[derive(State, Call, Query, Client)]
pub struct MultiCounter {
    pub counters: Map<Address, Counter>,
}

impl MultiCounter {
    #[call]
    pub fn increment(&mut self) -> Result<()> {
        let index = self.context::<Signer>().unwrap().signer.unwrap();
        self.counters.entry(index)?.or_default()?.increment();
        Ok(())
    }
}

impl BeginBlock for MultiCounter {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        for counter in self.counters.iter()? {
            let (index, counter) = counter?;
            println!("index: {:?}, count: {}", *index, counter.count)
        }

        Ok(())
    }
}
