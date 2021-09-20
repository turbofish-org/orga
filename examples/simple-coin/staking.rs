use super::SimpleCoin;
use orga::prelude::*;

#[derive(State, Call, Query)]
pub struct AppWithStaking {
    simp: SimpleCoin,
}
