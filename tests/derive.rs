// #![feature(trivial_bounds)]

// use orga::collections::Entry;
// use orga::collections::Next;
// use orga::encoding::{Decode, Encode};
// use orga::state::State;
// use orga::store::{MapStore, Shared, Store};

// #[derive(Encode, Decode, PartialEq, Debug)]
// struct Foo<T>
// where
//     T: Default,
// {
//     a: u8,
//     b: Option<T>,
// }

// #[test]
// fn encode_decode() {
//     let value = Foo { a: 5, b: Some(6) };
//     let bytes = value.encode().unwrap();
//     assert_eq!(bytes.as_slice(), &[5, 1, 0, 0, 0, 6]);
//     let decoded_value = Foo::decode(bytes.as_slice()).unwrap();
//     assert_eq!(decoded_value, value);
// }

// #[derive(State, Encode, Decode, Default)]
// struct MyStruct {
//     a: u32,
//     c: MyStruct2,
// }

// #[derive(State, Encode, Decode, Default)]
// struct MyStruct2(u32, u32);

// // #[derive(State)]
// // enum MyEnum {
// //     Unit,
// //     Tuple(u32, u32),
// //     Named { foo: u32 },
// // }

// #[test]
// fn struct_state() {
//     let mapstore = Shared::new(MapStore::new());
//     let store = Store::new(mapstore.into());

//     let mut state = MyStruct::default();
//     state.attach(store).unwrap();

//     assert_eq!(state.a, 0);
//     assert_eq!(state.c.0, 0);

//     state.a = 123;
//     state.c.0 = 5;
//     state.c.1 = 6;

//     state.flush().unwrap();
//     let bytes = state.encode().unwrap();
//     assert_eq!(bytes, vec![0, 0, 0, 123, 0, 0, 0, 5, 0, 0, 0, 6]);
// }

// #[derive(State, PartialEq, Debug, Encode, Decode, Default)]
// struct GenericStruct<T: State>
// where
//     T: Default,
// {
//     a: u8,
//     b: T,
// }

// #[test]
// fn generic_struct_state() {
//     let mapstore = Shared::new(MapStore::new());
//     let store = Store::new(mapstore.into());

//     let mut state: GenericStruct<u64> = Default::default();
//     state.attach(store).unwrap();
// }

// #[derive(Entry, Debug, PartialEq)]
// struct MyNamedStruct {
//     #[key]
//     my_key_1: u32,
//     #[key]
//     my_key_2: u16,
//     my_val: u8,
// }

// #[test]
// fn derive_entry_named_struct_into_entry() {
//     let test = MyNamedStruct {
//         my_key_1: 12,
//         my_key_2: 13,
//         my_val: 14,
//     };

//     assert_eq!(test.into_entry(), ((12, 13), (14,)));
// }

// #[test]
// fn derive_entry_named_struct_from_entry() {
//     let test = MyNamedStruct {
//         my_key_1: 12,
//         my_key_2: 13,
//         my_val: 14,
//     };

//     assert_eq!(MyNamedStruct::from_entry(((12, 13), (14,))), test);
// }

// #[derive(Entry, Debug, PartialEq)]
// struct TupleStruct(#[key] u8, u16, #[key] u32);

// #[test]
// fn derive_entry_tuple_struct_into_entry() {
//     let test = TupleStruct(8, 16, 32);

//     assert_eq!(test.into_entry(), ((8, 32), (16,)));
// }

// #[test]
// fn derive_entry_tuple_struct_from_entry() {
//     let test = TupleStruct(8, 16, 32);

//     assert_eq!(TupleStruct::from_entry(((8, 32), (16,))), test);
// }

// #[derive(Next, Debug, PartialEq)]
// struct NextStruct {
//     first_field: u8,
//     second_field: u8,
//     last_field: u8,
// }

// #[test]
// fn derive_next() {
//     let test_struct = NextStruct {
//         first_field: 0,
//         second_field: 0,
//         last_field: 0,
//     };
//     let expected = NextStruct {
//         first_field: 0,
//         second_field: 0,
//         last_field: 1,
//     };
//     assert_eq!(test_struct.next().unwrap(), expected);
// }

// #[test]
// fn derive_next_last() {
//     let test_struct = NextStruct {
//         first_field: 255,
//         second_field: 255,
//         last_field: 255,
//     };
//     assert!(test_struct.next().is_none());
// }

// #[derive(Next, Debug, PartialEq)]
// struct NextArrayStruct {
//     first_field: [u8; 2],
//     last_field: [u8; 2],
// }

// #[test]
// fn derive_next_array() {
//     let test = NextArrayStruct {
//         first_field: [0; 2],
//         last_field: [0; 2],
//     };

//     let expected = NextArrayStruct {
//         first_field: [0; 2],
//         last_field: [0, 1],
//     };

//     assert_eq!(test.next().unwrap(), expected);
// }

// #[test]
// fn derive_next_array_internal_last() {
//     let test = NextArrayStruct {
//         first_field: [0; 2],
//         last_field: [0, 255],
//     };

//     let expected = NextArrayStruct {
//         first_field: [0; 2],
//         last_field: [1, 0],
//     };

//     assert_eq!(test.next().unwrap(), expected);
// }

// #[test]
// fn derive_next_array_last() {
//     let test = NextArrayStruct {
//         first_field: [255, 255],
//         last_field: [255, 255],
//     };

//     assert!(test.next().is_none());
// }
