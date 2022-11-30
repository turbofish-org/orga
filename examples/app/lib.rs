use orga::{
    collections::Map,
    describe::{Builder, Describe, Descriptor},
    encoding::{Decode, Encode},
    state::State,
};
use wasm_bindgen::prelude::*;

#[derive(State, Encode, Decode)]
pub struct App {
    foo: u32,
    bar: u32,
    map: Map<u32, u32>,
}

impl Describe for App {
    fn describe() -> Descriptor {
        Builder::new::<Self>()
            .named_child::<u32>("foo", &[0], |v| Builder::access(v, |v: Self| v.foo))
            .named_child::<u32>("bar", &[1], |v| Builder::access(v, |v: Self| v.bar))
            .named_child::<Map<u32, u32>>("foo", &[0], |v| Builder::access(v, |v: App| v.foo))
            .build()
    }
}

#[wasm_bindgen]
pub fn describe() -> Descriptor {
    App::describe()
}
