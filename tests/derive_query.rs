#![feature(specialization)]
#![feature(trivial_bounds)]

use orga::collections::Deque;
use orga::query::Query;

#[derive(Query)]
struct Foo<T> {
    pub a: u8,
    _a2: u8,
    pub b: Option<T>,
    pub bar: Bar,
}

#[derive(Query)]
pub struct Bar {
    deque: Deque<u32>,
}

impl<T> Foo<T> {
    #[query]
    pub fn basic(&self) {}

    #[query]
    pub fn input_and_output(&self, n: u32) -> u32 {
        self.a as u32 + n
    }

    #[query]
    pub fn generic_input(&self, _t: T) -> u32 {
        123
    }

    #[query]
    pub fn complex_type(&self) -> orga::Result<u32> {
        let res = self.bar.deque.get(123)?.unwrap_or_default();
        Ok(*res)
    }
}

impl<T: Clone + Default> Foo<T> {
    #[query]
    pub fn generic_output(&self) -> T {
        self.b.clone().unwrap_or_default()
    }

    #[query]
    pub fn wrapped_generic_output(&self) -> Option<T> {
        self.b.clone()
    }
}

fn _assert_type<T>(_: T) {}

fn _exhaustive_match<T: Query>(query: foo_query::Query<T>) {
    use foo_query::Query;
    match query {
        Query::This => {}
        Query::FieldA(subquery) => _assert_type::<()>(subquery),
        Query::FieldB(subquery) => _assert_type::<T::Query>(subquery),
        Query::FieldBar(subquery) => _assert_type::<<Bar as orga::query::Query>::Query>(subquery),
        Query::MethodBasic(subquery) => _assert_type::<Vec<u8>>(subquery),
        Query::MethodInputAndOutput(n, subquery) => {
            _assert_type::<u32>(n);
            _assert_type::<Vec<u8>>(subquery);
        }
        Query::MethodGenericInput(t, subquery) => {
            _assert_type::<T>(t);
            _assert_type::<Vec<u8>>(subquery);
        }
        Query::MethodComplexType(subquery) => _assert_type::<Vec<u8>>(subquery),
        Query::MethodGenericOutput(subquery) => _assert_type::<Vec<u8>>(subquery),
        Query::MethodWrappedGenericOutput(subquery) => _assert_type::<Vec<u8>>(subquery),
    }
}