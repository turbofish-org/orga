use crate::Error;
use std::any::TypeId;
use std::cell::RefCell;

thread_local! {
    static TRACING: RefCell<bool> = RefCell::new(false);
}

pub fn tracing() -> bool {
    TRACING.with(|tracing| *tracing.borrow())
}

pub fn set_tracing(enabled: bool) {
    TRACING.set(enabled);
}

pub struct Guard;

pub fn tracing_guard() -> Guard {
    set_tracing(true);
    Guard
}

impl Drop for Guard {
    fn drop(&mut self) {
        set_tracing(false);
    }
}

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

pub fn maybe_push_trace<T: 'static, F: FnOnce() -> (MethodType, Vec<u8>, Vec<u8>)>(
    op: F,
    // method_type: MethodType,
    // method_prefix: Vec<u8>,
    // method_args: Vec<u8>,
) {
    if !tracing() {
        return;
    }
    let type_id = TypeId::of::<T>();
    let (method_type, method_prefix, method_args) = op();

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

// pub fn maybe_push_trace<F: FnOnce()>(op: F) {
//     if tracing() {
//         op()
//     }
// }

pub fn maybe_pop_trace<T, F: FnOnce() -> T>(op: F) -> T {
    if !tracing() {
        return op();
    }
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
