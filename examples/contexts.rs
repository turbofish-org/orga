#![feature(specialization)]
#![feature(trivial_bounds)]
#![feature(associated_type_defaults)]
use orga::prelude::*;

#[derive(State, Call, Query)]
pub struct MyApp {
    num_blocks: u32,
}

impl BeginBlock for MyApp {
    fn begin_block(&mut self) -> Result<()> {
        self.num_blocks += 1;
        println!("num_blocks: {}", self.num_blocks);
        Ok(())
    }
}

fn main() {
    Node::<SignerProvider<MyApp>>::new("contexts_app")
        .reset()
        .run();
}
