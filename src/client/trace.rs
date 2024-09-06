//! Call and query tracing.
use crate::Error;
use std::any::TypeId;
use std::cell::RefCell;

thread_local! {
    static TRACING: RefCell<bool> = const { RefCell::new(false) };
}

/// Returns true if tracing is enabled.
pub fn tracing() -> bool {
    TRACING.with(|tracing| *tracing.borrow())
}

/// Set tracing to the given value.
pub fn set_tracing(enabled: bool) {
    TRACING.set(enabled);
}

/// A guard that enables tracing when created and disables it on drop.
pub struct Guard;

/// Creates a new tracing guard.
pub fn tracing_guard() -> Guard {
    set_tracing(true);
    Guard
}

impl Drop for Guard {
    fn drop(&mut self) {
        set_tracing(false);
    }
}

/// The type of method that's being traced.
#[derive(Debug, Clone)]
pub enum MethodType {
    /// A query method.
    Query,
    /// A call method.
    Call,
}

/// A single trace entry.
#[derive(Debug, Clone)]
pub struct Trace {
    /// The type ID for the type whose method was called.
    pub type_id: TypeId,
    /// The type of method that was called.
    pub method_type: MethodType,
    /// The local call or query prefix for the method that was called.
    pub method_prefix: Vec<u8>,
    /// The encoded arguments to the traced method call.
    pub method_args: Vec<u8>,
}

impl Trace {
    /// Returns the full bytes for the trace.
    pub fn bytes(&self) -> Vec<u8> {
        [self.method_prefix.clone(), self.method_args.clone()].concat()
    }
}

/// The state of the trace stack.
#[derive(Debug, Default, Clone)]
pub struct TraceState {
    /// The current stack of traces.
    pub stack: Vec<Trace>,
    /// Previous traces.
    pub history: Vec<Trace>,
}

thread_local! {
    static TRACE: RefCell<TraceState> = RefCell::new(TraceState {
        stack: vec![],
        history: vec![],
    });
}

/// Pushes a trace onto the stack if tracing is enabled.
pub fn maybe_push_trace<T: 'static, F: FnOnce() -> (MethodType, Vec<u8>, Vec<u8>)>(op: F) {
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

/// Pops a trace from the stack if tracing is enabled.
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

/// Takes the current trace state, leaving an empty one in its place.
pub fn take_trace() -> TraceState {
    TRACE.take()
}
