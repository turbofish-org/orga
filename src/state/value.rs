use std::borrow::Borrow;
use std::marker::PhantomData;

use failure::Fail;

use crate::encoding::{Decode, Encode};
use crate::state::{State, Query};
use crate::store::{Read, Store};

const EMPTY_KEY: &[u8] = &[];

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Value does not exist")]
    NotFound,

    #[fail(display = "{}", _0)]
    Other(failure::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Self {
        Error::Other(err)
    }
}

/// A simple implementation of `State` which gets or sets a single value,
/// automatically decoding when getting and encoding when setting.
pub struct Value<S: Read, T: Encode + Decode> {
    store: S,
    value_type: PhantomData<T>,
}

impl<S: Read, T: Encode + Decode> State<S> for Value<S, T> {
    /// Constructs a `Value` by wrapping a store instance.
    fn wrap_store(store: S) -> std::result::Result<Value<S, T>, failure::Error> {
        Ok(Value {
            store,
            value_type: PhantomData,
        })
    }
}

impl<S: Read, T: Encode + Decode> Value<S, T> {
    /// Gets the stored value, erroring if it does not yet exist.
    pub fn get(&self) -> Result<T> {
        match self.maybe_get()? {
            Some(value) => Ok(value),
            None => Err(Error::NotFound),
        }
    }

    /// Gets the stored value, returning `None` if it does not yet exist.
    pub fn maybe_get(&self) -> Result<Option<T>> {
        match self.store.get(EMPTY_KEY)? {
            Some(bytes) => Ok(Some(T::decode(bytes.as_slice())?)),
            None => Ok(None),
        }
    }
}

impl<S: Read, T: Encode + Decode> Query for Value<S, T> {
    type Request = ();
    type Response = Option<T>;

    fn query(&self, _: ()) -> crate::Result<Option<T>> {
        Ok(self.maybe_get()?)
    }

    fn resolve(&self, _: ()) -> crate::Result<()> {
        self.store.get(&[])?;
        Ok(())
    }
}

impl<S: Read, T: Encode + Decode + Default> Value<S, T> {
    /// Gets the stored value, returning the default if it does not yet exist.
    pub fn get_or_default(&self) -> Result<T> {
        match self.get() {
            Ok(value) => Ok(value),
            Err(err) => {
                if let Error::NotFound = err {
                    return Ok(T::default());
                }
                Err(err)
            }
        }
    }
}

impl<S: Store, T: Encode + Decode> Value<S, T> {
    /// Sets the value and writes it to the store, replacing it if it already
    /// exists.
    pub fn set<B: Borrow<T>>(&mut self, value: B) -> Result<()> {
        let bytes = value.borrow().encode()?;
        Ok(self.store.put(EMPTY_KEY.to_vec(), bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::Value;
    use crate::{
        state::State,
        store::{MapStore, Read},
    };

    #[test]
    fn u64_wrapper() {
        let mut store = MapStore::new();

        {
            let mut n: Value<_, u64> = Value::wrap_store(&mut store).unwrap();

            assert_eq!(n.get().unwrap_err().to_string(), "Value does not exist");
            assert_eq!(n.maybe_get().unwrap(), None);

            n.set(0x1234567890u64).unwrap();
            assert_eq!(n.get().unwrap(), 0x1234567890);
            assert_eq!(n.maybe_get().unwrap(), Some(0x1234567890u64));
        }

        assert_eq!(
            store.get(&[]).unwrap(),
            Some(vec![0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90])
        );
    }

    #[test]
    fn default() {
        let mut store = MapStore::new();
        let n = Value::<_, u64>::wrap_store(&mut store).unwrap();
        assert_eq!(n.get_or_default().unwrap(), 0);
    }

    #[test]
    fn read_only() {
        let mut store = MapStore::new();
        let mut n: Value<_, u64> = (&mut store).wrap().unwrap();
        n.set(123).unwrap();

        let read = store;
        let n: Value<_, u64> = read.wrap().unwrap();
        assert_eq!(n.get_or_default().unwrap(), 123);
        assert_eq!(n.get().unwrap(), 123);
        assert_eq!(n.maybe_get().unwrap(), Some(123));
    }
}
