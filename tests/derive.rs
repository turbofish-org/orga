// #![cfg(integration_test)]
#![feature(trivial_bounds)]

use orga::collections::Entry;
use orga::encoding::{Decode, Encode};
use orga::macros::Entry;
use orga::state::State;
use orga::store::Shared;
use orga::store::{MapStore, Store};
use orga::query::Query;

#[derive(Encode, Decode, PartialEq, Debug, Query)]
struct Foo {
    a: u8,
    b: Option<u8>,
}

impl Foo {
    fn x() {}

    #[query]
    pub fn some_method(&self) {}

    #[query]
    pub fn z(&self, n: u32) -> u32 {
        123 + n
    }
}

impl Foo {
    #[query]
    pub fn y2(&self) {}

    #[query]
    fn z2(&self) -> u32 {
        123
    }

    #[query]
    pub fn z3(&self) -> u32 {
        123
    }
}

#[test]
fn encode_decode() {
    let value = Foo { a: 5, b: Some(6) };
    let bytes = value.encode().unwrap();
    assert_eq!(bytes.as_slice(), &[5, 1, 6]);
    let decoded_value = Foo::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded_value, value);
}

#[derive(State)]
struct MyStruct {
    a: u32,
    c: MyStruct2,
}

#[derive(State)]
struct MyStruct2(u32, u32);

// #[derive(State)]
// enum MyEnum {
//     Unit,
//     Tuple(u32, u32),
//     Named { foo: u32 },
// }

// fn struct_state() {
//     let mapstore = MapStore::new();
//     let store = Store::new(mapstore);

//     let mut state = MyStruct::create(store.clone(), Default::default()).unwrap();

//     assert_eq!(state.a, 0);
//     assert_eq!(state.c.0, 0);

//     state.a = 123;
//     state.c.0 = 5;
//     state.c.1 = 6;

//     let data = state.flush().unwrap();
//     let bytes = data.encode().unwrap();
//     assert_eq!(bytes, vec![0, 0, 0, 123, 0, 0, 0, 5, 0, 0, 0, 6]);
// }

#[derive(Entry, Debug, PartialEq)]
struct MyNamedStruct {
    #[key]
    my_key_1: u32,
    #[key]
    my_key_2: u16,
    my_val: u8,
}

#[test]
fn derive_entry_named_struct_into_entry() {
    let test = MyNamedStruct {
        my_key_1: 12,
        my_key_2: 13,
        my_val: 14,
    };

    assert_eq!(test.into_entry(), ((12, 13), (14,)));
}

#[test]
fn derive_entry_named_struct_from_entry() {
    let test = MyNamedStruct {
        my_key_1: 12,
        my_key_2: 13,
        my_val: 14,
    };

    assert_eq!(MyNamedStruct::from_entry(((12, 13), (14,))), test);
}

#[derive(Entry, Debug, PartialEq)]
struct TupleStruct(#[key] u8, u16, #[key] u32);

#[test]
fn derive_entry_tuple_struct_into_entry() {
    let test = TupleStruct(8, 16, 32);

    assert_eq!(test.into_entry(), ((8, 32), (16,)));
}

#[test]
fn derive_entry_tuple_struct_from_entry() {
    let test = TupleStruct(8, 16, 32);

    assert_eq!(TupleStruct::from_entry(((8, 32), (16,))), test);
}
