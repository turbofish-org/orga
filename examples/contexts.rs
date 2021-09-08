#![feature(specialization)]
#![feature(trivial_bounds)]
#![feature(associated_type_defaults)]
use orga::call::Call;
use orga::prelude::*;

#[derive(State)]
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

impl Call for MyApp {
    type Call = ();
    fn call(&mut self, _call: Self::Call) -> Result<()> {
        Ok(())
    }
}

fn main() {
    Node::<SignerProvider<MyApp>>::new("contexts_app")
        .reset()
        .run();
}
