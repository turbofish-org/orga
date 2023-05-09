use crate::{Error, Result};
use std::any::TypeId;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct Trace {
    pub type_id: TypeId,
    pub method_prefix: Vec<u8>,
    pub method_args: Vec<u8>,
}

impl Trace {
    pub fn bytes(&self) -> Vec<u8> {
        vec![self.method_prefix.clone(), self.method_args.clone()].concat()
    }
}

thread_local! {
    static TRACE: RefCell<(Vec<Trace>, u64)> = RefCell::new((vec![], 0));
}

pub fn push_trace<T: 'static>(method_prefix: Vec<u8>, method_args: Vec<u8>) -> Result<()> {
    let type_id = TypeId::of::<T>();

    TRACE.with(|traces| {
        let mut traces = traces
            .try_borrow_mut()
            .map_err(|_| Error::Call("Call tracer is already borrowed".to_string()))?;

        traces.0.push(Trace {
            type_id,
            method_prefix,
            method_args,
        });

        traces.1 += 1;

        Result::Ok(())
    })
}

pub fn maybe_pop_trace<F: FnOnce() -> std::result::Result<T, E>, T, E>(
    op: F,
) -> std::result::Result<T, E> {
    let res = op();
    if res.is_ok() {
        TRACE.with(|traces| {
            let mut traces = traces.try_borrow_mut().unwrap(); // TODO
            traces.0.pop();
        })
    }
    res
}

pub fn take_trace() -> (Vec<Trace>, u64) {
    TRACE.take()
}
