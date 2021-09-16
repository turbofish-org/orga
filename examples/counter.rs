#![feature(min_specialization)]
use orga::prelude::*;

#[derive(State, Call, Query)]
pub struct Counter {
    num_blocks: u32,
}

impl BeginBlock for Counter {
    fn begin_block(&mut self, _ctx: &BeginBlockCtx) -> Result<()> {
        self.num_blocks += 1;
        println!("num_blocks is {}", self.num_blocks);
        Ok(())
    }
}

fn main() {
    Node::<Counter>::new("my_counter").reset().run();
}
