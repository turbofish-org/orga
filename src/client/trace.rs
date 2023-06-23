use crate::Error;
use std::any::TypeId;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub enum MethodType {
    Query,
    Call,
}

#[derive(Debug, Clone)]
pub struct Trace {
    pub type_id: TypeId,
    pub method_type: MethodType,
    pub method_prefix: Vec<u8>,
    pub method_args: Vec<u8>,
}

impl Trace {
    pub fn bytes(&self) -> Vec<u8> {
        vec![self.method_prefix.clone(), self.method_args.clone()].concat()
    }
}

#[derive(Debug, Default, Clone)]
pub struct TraceState {
    pub stack: Vec<Trace>,
    pub history: Vec<Trace>,
}

thread_local! {
    static TRACE: RefCell<TraceState> = RefCell::new(TraceState {
        stack: vec![],
        history: vec![],
    });
}

pub fn push_trace<T: 'static>(
    method_type: MethodType,
    method_prefix: Vec<u8>,
    method_args: Vec<u8>,
) {
    let type_id = TypeId::of::<T>();

    TRACE.with(|traces| {
        let mut traces = traces
            .try_borrow_mut()
            .map_err(|_| Error::Call("Call tracer is already borrowed".to_string()))
            .unwrap();

        let trace = Trace {
            type_id,
            method_type,
            method_prefix,
            method_args,
        };

        if traces.stack.is_empty() {
            traces.history.push(trace.clone());
        }

        traces.stack.push(trace);
    })
}

pub fn maybe_pop_trace<T, F: FnOnce() -> T>(op: F) -> T {
    let res = op();
    TRACE.with(|traces| {
        let mut traces = traces.try_borrow_mut().unwrap(); // TODO
        traces.stack.pop();
    });
    res
}

pub fn take_trace() -> TraceState {
    TRACE.take()
}
