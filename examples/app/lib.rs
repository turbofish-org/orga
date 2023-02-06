// use orga::{
//     collections::Map,
//     describe::{Describe, Descriptor},
//     encoding::{Decode, Encode},
//     state::State,
// };
// use serde::{Deserialize, Serialize};
// use wasm_bindgen::prelude::*;

// #[derive(State, Encode, Decode, Describe, Serialize, Deserialize)]
// pub struct App {
//     foo: u32,
//     bar: u32,
//     map: Map<u32, u32>,
// }

// #[wasm_bindgen]
// pub fn describe() -> Descriptor {
//     App::describe()
// }
