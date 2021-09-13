#![feature(specialization)]
#![feature(trivial_bounds)]
#![feature(associated_type_defaults)]
use orga::prelude::*;

#[derive(State, Call, Query)]
pub struct MyApp {
    height: u64,
}

impl BeginBlock for MyApp {
    fn begin_block(&mut self, ctx: &BeginBlockCtx) -> Result<()> {
        self.height = ctx.height;
        println!("height: {}", self.height);
        Ok(())
    }
}

impl EndBlock for MyApp {
    fn end_block(&mut self, ctx: &EndBlockCtx) -> Result<()> {
        ctx.set_voting_power([0; 32], 2);

        Ok(())
    }
}

fn main() {
    Node::<SignerProvider<MyApp>>::new("contexts_app")
        .reset()
        .run();
}
