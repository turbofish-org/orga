use crate::{Decode, Encode, Store, State};
use failure::Fail;
use std::borrow::Borrow;
use std::marker::PhantomData;

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

pub struct Value<S: Store, T: Encode + Decode> {
    store: S,
    value_type: PhantomData<T>,
}

impl<S: Store, T: Encode + Decode> State<S> for Value<S, T> {
    fn wrap_store(store: S) -> std::result::Result<Value<S, T>, failure::Error> {
        Ok(Value {
            store,
            value_type: PhantomData,
        })
    }
}

impl<S: Store, T: Encode + Decode> Value<S, T> {
    pub fn get(&self) -> Result<T> {
        match self.maybe_get()? {
            Some(value) => Ok(value),
            None => Err(Error::NotFound),
        }
    }

    pub fn maybe_get(&self) -> Result<Option<T>> {
        match self.store.get(EMPTY_KEY)? {
            Some(bytes) => Ok(Some(T::decode(bytes.as_slice())?)),
            None => Ok(None),
        }
    }

    pub fn set<B: Borrow<T>>(&mut self, value: B) -> Result<()> {
        let bytes = value.borrow().encode()?;
        Ok(self.store.put(EMPTY_KEY.to_vec(), bytes)?)
    }
}

impl<S: Store, T: Encode + Decode + Default> Value<S, T> {
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

#[cfg(test)]
mod tests {
    use super::Value;
    use crate::{MapStore, Read, State};

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
}
