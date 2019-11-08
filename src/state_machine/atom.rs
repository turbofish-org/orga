use crate::error::Result;
use crate::store::{WriteCache, Flush, Store};

pub fn step_atomic<'a, F, S: Store, I, O>(
    sm: F,
    store: &'a mut S,
    input: I
) -> Result<O>
    where F: Fn(&mut WriteCache<'a, S>, I) -> Result<O> + 'a
{
    let mut flush_store = WriteCache::wrap(store);
    let res = sm(&mut flush_store, input)?;
    flush_store.flush()?;
    Ok(res)
}

pub fn bind_atomic<'a, F, S: Store, I, O>(
    sm: F,
    store: &'a mut S
) -> impl FnMut(I) -> Result<O> + 'a
    where F: Fn(&mut WriteCache<S>, I) -> Result<O> + 'a
{
    move |input| step_atomic(&sm, store, input)
}
