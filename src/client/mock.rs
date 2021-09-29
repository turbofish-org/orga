use std::sync::{Arc, Mutex};
use super::AsyncCall;
use crate::call::Call;
use crate::Result;

pub struct Mock<T>(pub Arc<Mutex<T>>);

impl<T> Clone for Mock<T> {
    fn clone(&self) -> Self {
        Mock(self.0.clone())
    }
}

#[async_trait::async_trait]
impl<T: Call> AsyncCall for Mock<T>
where
    T: Send + Sync,
    T::Call: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        self.0.lock().unwrap().call(call)
    }
}
