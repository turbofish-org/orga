use orga::encoding::{Decode, Encode};
use orga::state::State;
use orga::store::{MapStore, Store};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Foo {
    a: u8,
    b: Option<u8>,
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

#[test]
fn struct_state() {
    let mapstore = MapStore::new();
    let store = Store::new(mapstore);

    let mut state = MyStruct::create(store.clone(), Default::default()).unwrap();

    assert_eq!(state.a, 0);
    assert_eq!(state.c.0, 0);

    state.a = 123;
    state.c.0 = 5;
    state.c.1 = 6;

    let data = state.flush().unwrap();
    let bytes = data.encode().unwrap();
    assert_eq!(bytes, vec![0, 0, 0, 123, 0, 0, 0, 5, 0, 0, 0, 6]);
}
