#![allow(incomplete_features)]
#![feature(specialization)]
#![feature(trivial_bounds)]
#![feature(auto_traits)]
#![feature(negative_impls)]

// use std::ops::Add;

// use orga::call::{Call, FieldCall};
// use orga::collections::Deque;
// use orga::query::Query;

// #[derive(Query, FieldCall, Debug)]
// struct Foo<T> {
//     #[call]
//     pub a: u8,
//     _a2: u8,
//     // #[call]
//     pub b: Option<T>,
//     #[call]
//     pub bar: Bar,
// }

// #[derive(Query, FieldCall, Debug)]
// pub struct Bar {
//     deque: Deque<u32>,
// }

// #[derive(Query, FieldCall, Debug)]
// pub struct Baz<T: Clone>
// where
//     T: Add,
// {
//     #[call]
//     deque: Deque<u32>,
//     _marker: std::marker::PhantomData<fn(T)>,
// }

// #[derive(Query, Call)]
// pub struct TupleStruct(#[call] u32);

// #[orga]
// impl<T: 'static> Foo<T> {
//     pub fn _no_attr(&self) {}

//     #[query]
//     pub fn basic(&self) {}

//     #[query]
//     pub fn input_and_output(&self, n: u32) -> u32 {
//         self.a as u32 + n
//     }

//     #[query]
//     pub fn ref_output(&self) -> &u32 {
//         &123
//     }

//     // TODO:
//     // #[query]
//     // pub fn ref_input(&self, _n: &u32) {}

//     #[query]
//     pub fn generic_input(&self, _t: T) -> u32 {
//         123
//     }

//     #[query]
//     pub fn complex_type(&self) -> orga::Result<u32> {
//         let res = self.bar.deque.get(123)?.unwrap_or_default();
//         Ok(*res)
//     }

//     // #[call]
//     // pub fn basic_call(&mut self) {}

//     // #[call]
//     pub fn _input_and_output_call(&mut self, n: u32) -> u32 {
//         self.a as u32 + n
//     }

//     // TODO:
//     // #[call]
//     // pub fn ref_input_call(&self, _n: &u32) {}

//     // #[call]
//     pub fn _generic_input_call(&mut self, _t: T) -> u32 {
//         123
//     }

//     #[query]
//     pub fn wrapped_generic_output(&self) -> Option<&T> {
//         self.b.as_ref()
//     }

//     #[call]
//     pub fn complex_type_call(
//         &mut self,
//     ) -> orga::Result<Option<orga::collections::ChildMut<u64, u32>>> {
//         self.bar.deque.get_mut(123)
//     }

//     //#[call]
//     pub fn _generic_output_call(&mut self) -> Option<&mut T> {
//         self.b.as_mut()
//     }
// }

// impl<T: Clone + Default> Foo<T> {
//     #[query]
//     pub fn generic_output(&self) -> T {
//         self.b.clone().unwrap_or_default()
//     }
// }

// fn _assert_type<T>(_: T) {}

// fn _exhaustive_match_query<T: Query>(query: foo_query::Query<T>) {
//     use foo_query::Query;
//     match query {
//         Query::This => {}
//         Query::FieldA(subquery) => _assert_type::<()>(subquery),
//         Query::FieldB(subquery) => _assert_type::<T::Query>(subquery),
//         Query::FieldBar(subquery) =>
// _assert_type::<bar_query::Query>(subquery),
//         Query::MethodBasic(subquery) => _assert_type::<Vec<u8>>(subquery),
//         Query::MethodInputAndOutput(n, subquery) => {
//             _assert_type::<u32>(n);
//             _assert_type::<Vec<u8>>(subquery);
//         }
//         Query::MethodRefOutput(subquery) => {
//             _assert_type::<Vec<u8>>(subquery);
//         }
//         Query::MethodGenericInput(t, subquery) => {
//             _assert_type::<T>(t);
//             _assert_type::<Vec<u8>>(subquery);
//         }
//         Query::MethodComplexType(subquery) =>
// _assert_type::<Vec<u8>>(subquery),
//         Query::MethodGenericOutput(subquery) =>
// _assert_type::<Vec<u8>>(subquery),
//         Query::MethodWrappedGenericOutput(subquery) =>
// _assert_type::<Vec<u8>>(subquery),     }
// }

// fn _exhaustive_match_call<T: Call>(call: foo_call::Call<T>) {
//     use foo_call::Call;
//     match call {
//         Call::Noop => {}
//         Call::FieldA(subcall) => _assert_type::<Vec<u8>>(subcall),
//         Call::FieldB(subcall) => _assert_type::<Vec<u8>>(subcall),
//         Call::FieldBar(subcall) => _assert_type::<Vec<u8>>(subcall),
//         Call::MethodBasicCall(subcall) => _assert_type::<Vec<u8>>(subcall),
//         Call::MethodInputAndOutputCall(n, subcall) => {
//             _assert_type::<u32>(n);
//             _assert_type::<Vec<u8>>(subcall);
//         }
//         Call::MethodGenericInputCall(t, subcall) => {
//             _assert_type::<T>(t);
//             _assert_type::<Vec<u8>>(subcall);
//         }
//         Call::MethodComplexTypeCall(subcall) =>
// _assert_type::<Vec<u8>>(subcall),
//         Call::MethodGenericOutputCall(subcall) =>
// _assert_type::<Vec<u8>>(subcall),     }
// }

// #[test]
// fn query_call_debug() {
//     let query = <Foo<i32> as
// orga::query::Query>::Query::MethodGenericInput(123, vec![42]);     println!("
// {:?}", query);

//     let call = <Foo<i32> as
// orga::call::Call>::Call::MethodGenericInputCall(234, vec![42]);     println!
// ("{:?}", call); }
