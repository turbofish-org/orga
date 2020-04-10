use std::borrow::Borrow;
use std::marker::PhantomData;
use failure::bail;
use crate::{Encode, Decode, Store, Result, WrapStore};

const EMPTY_KEY: &[u8] = &[];

pub struct StateValue<'a, T: Encode + Decode> {
    store: &'a mut dyn Store,
    value_type: PhantomData<T>
}

impl<'a, T: Encode + Decode> WrapStore<'a, StateValue<'a, T>> for T {
    fn wrap_store(store: &'a mut dyn Store) -> StateValue<'a, T> {
        StateValue { store, value_type: PhantomData }
    }
}

impl<'a, T: Encode + Decode> StateValue<'a, T> {
    fn get(&self) -> Result<T> {
        match self.store.get(EMPTY_KEY)? {
            Some(bytes) => T::decode(bytes.as_slice()),
            None => bail!("Value does not exist")
        }
    }

    fn set<B: Borrow<T>>(&mut self, value: B) -> Result<()> {
        let bytes = value.borrow().encode()?;
        self.store.put(EMPTY_KEY.to_vec(), bytes)
    }
}

#[test]
fn wrap_store() {
    use crate::{MapStore, Read};

    let mut store = MapStore::new();
    let mut n = u64::wrap_store(&mut store);

    assert_eq!(
        n.get().unwrap_err().to_string(),
        "Value does not exist"
    );

    n.set(0x1234567890u64).unwrap();
    assert_eq!(n.get().unwrap(), 0x1234567890);
    assert_eq!(
        store.get(EMPTY_KEY).unwrap(),
        Some(vec![0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x90])
    );
}
