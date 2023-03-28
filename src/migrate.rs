use crate::{prelude::State, Result};
use std::path::Path;

pub trait Migrate<T: State> {
    fn migrate(&mut self, legacy: T) -> Result<()>;
}

pub fn exec_migration<T: Migrate<U>, U: State, P: AsRef<Path>>(
    state: &mut T,
    old_store_path: P,
    prefix: &[u8],
) -> Result<()> {
    unimplemented!()
}
