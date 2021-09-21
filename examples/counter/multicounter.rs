use super::Counter;
use orga::{coins::Address, prelude::*};

#[derive(State, Call, Query)]
pub struct MultiCounter {
    pub counters: Map<Address, Counter>,
}
