use crate::error::Result;
use crate::store::{WriteCache, Flush, Store};
use super::StateMachine;

pub fn step_atomic<'a, S: Store, I, O>(
    sm: impl StateMachine<WriteCache<'a, S>, I, O>,
    store: &'a mut S,
    input: I
) -> Result<O> {
    let mut flush_store = WriteCache::wrap(store);
    let res = sm(&mut flush_store, input)?;
    flush_store.flush()?;
    Ok(res)
}
