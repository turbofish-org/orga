use std::rc::Rc;
use std::cell::RefCell;

use crate::Result;
use crate::call::Call;
use crate::query::Query;
use crate::state::State;
use super::{Client, CreateClient};

pub struct Mock<T> {
    state: Rc<RefCell<T>>,
}

impl<T> Clone for Mock<T> {
    fn clone(&self) -> Mock<T> {
        Mock { state: self.state.clone() }
    }
}

impl<T: State> Mock<T> {
    pub fn new(state: T) -> (Mock<T>, Rc<RefCell<T>>) {
        let shared = Rc::new(RefCell::new(state));
        (Mock { state: shared.clone() }, shared)
    }

    // pub fn setup() -> (Mock<T>, Rc<RefCell<T>>) {
    //     let shared = Rc::new(RefCell::new(state));
    //     (Mock { state: shared.clone() }, shared)
    // }
}

impl<T: Call + Query> Client<T> for Mock<T> {
    fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
    where F: Fn(T::Res) -> Result<R> {
        let state = self.state.borrow();
        check(state.query(query)?)
    }

    fn call(&mut self, call: T::Call) -> Result<()> {
        let mut state = self.state.borrow_mut();
        state.call(call)
    }
}
