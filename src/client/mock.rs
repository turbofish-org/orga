use super::{AsyncCall, Client};
use crate::call::Call;
use crate::query::Query;
use crate::Result;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

pub struct Mock<T: Client<Adapter<T>>>(T::Client, Arc<Mutex<T>>);

impl<T: Client<Adapter<T>>> Mock<T> {
    pub fn new(state: Arc<Mutex<T>>) -> Mock<T> {
        let client = T::create_client(Adapter(state.clone()));
        Mock(client, state)
    }
}

impl<T: Client<Adapter<T>> + Query> Mock<T> {
    pub fn query<F, R>(&self, _query: T::Query, check: F) -> Result<R>
    where
        F: Fn(&T) -> Result<R>,
    {
        let state = self.1.lock().unwrap();
        check(&*state)
    }
}

impl<T: Client<Adapter<T>>> Deref for Mock<T> {
    type Target = T::Client;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Client<Adapter<T>>> DerefMut for Mock<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Adapter<T>(Arc<Mutex<T>>);

impl<T> Clone for Adapter<T> {
    fn clone(&self) -> Self {
        Adapter(self.0.clone())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Call> AsyncCall for Adapter<T>
where
    T: Send + Sync,
    T::Call: Send,
{
    type Call = T::Call;

    async fn call(&mut self, call: Self::Call) -> Result<()> {
        self.0.lock().unwrap().call(call)
    }
}
