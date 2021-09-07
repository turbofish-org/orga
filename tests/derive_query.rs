#![feature(specialization)]
#![feature(trivial_bounds)]

use orga::collections::Deque;
use orga::query::Query;

#[derive(Query)]
struct Foo<T> {
    a: u8,
    b: Option<T>,
    bar: Bar,
}

#[derive(Query)]
struct Bar {
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
