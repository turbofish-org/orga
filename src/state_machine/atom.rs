use crate::error::Result;
use crate::store::{WriteCache, Flush, Store};

pub fn step_atomic<'a, S, T, I, O>(sm: T, store: &'a mut S, input: I) -> Result<O>
    where
        S: Store,
        T: Fn(&mut WriteCache<'a, S>, I) -> Result<O>
{
    let mut flush_store = WriteCache::wrap(store);
    let res = sm(&mut flush_store, input)?;
    flush_store.flush()?;
    Ok(res)
}
