use crate::{store::Store, Result};

pub mod exec;
pub mod mock;
pub mod trace;
pub mod wallet;

pub trait Client {
    async fn query(&self, query: &[u8]) -> Result<Store>;

    async fn call(&self, call: &[u8]) -> Result<()>;
}
