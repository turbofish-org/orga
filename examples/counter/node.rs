use super::CounterApp;
use orga::prelude::*;

pub fn run_node() {
    Node::<CounterApp>::new("my_counter").reset().run();
}
