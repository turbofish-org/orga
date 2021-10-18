use orga::prelude::*;

#[derive(State, Call, Query, Client)]
pub struct Counter {
    num_blocks: u32,
    pub count: u64,
}

impl BeginBlock for Counter {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        self.num_blocks += 1;
        println!("num_blocks is {}, count is {}", self.num_blocks, self.count);
        Ok(())
    }
}

impl Counter {
    #[call]
    pub fn increment(&mut self) {
        self.count += 1;
    }
}
