use std::{
    io::{BufRead, BufReader, Read, Write},
    iter::Peekable,
};

use serde::Deserialize;

use crate::{
    describe::{Children, Describe, Descriptor, Value},
    state::State,
    store::{Iter, Store},
    Error, Result,
};

pub fn migrate<R: std::io::Read, T: State>(version: u32, input: R, mut store: Store) -> Result<()>
where
    for<'de> T: Deserialize<'de>,
{
    use crate::store::Write;

    let mut value: serde_json::Value = serde_json::from_reader(input)?;
    T::transform(version, &mut value)?;
    let mut state: T = serde_json::from_value(value)?;
    state.attach(store.clone())?;
    state.flush()?;
    store.put(vec![], state.encode()?)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use crate::{
        collections::Map,
        describe::{Describe, Value},
        encoding::{Decode, Encode},
        state::State,
        store::{DefaultBackingStore, MapStore, Shared, Store, Write},
        Result,
    };

    #[derive(State, Encode, Decode, Describe, Default, Serialize, Deserialize)]
    struct Foo {
        bar: u32,
        baz: Map<u32, u32>,
        beep: Map<u32, Map<bool, u8>>,
    }

    fn create_foo_value() -> Result<Value> {
        let mut store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));

        let mut foo = Foo::default();
        // TODO: remove this prefix after ABCI refactor
        foo.attach(store.sub(&[0]))?;

        foo.baz.insert(123, 456)?;
        foo.baz.insert(789, 1)?;
        foo.baz.insert(1000, 2)?;
        foo.baz.insert(1001, 3)?;

        foo.beep.insert(10, Map::new())?;
        foo.beep.get_mut(10)?.unwrap().insert(true, 1)?;
        foo.beep.get_mut(10)?.unwrap().insert(false, 0)?;
        foo.beep.insert(20, Map::new()).unwrap();
        foo.beep.get_mut(20)?.unwrap().insert(true, 0)?;
        foo.beep.get_mut(20)?.unwrap().insert(false, 1)?;

        foo.flush()?;
        store.put(vec![], foo.encode()?)?;

        let mut value = Value::new(foo);
        // TODO: remove this prefix after ABCI refactor
        value.attach(store.sub(&[0]))?;

        Ok(value)
    }

    #[test]
    pub fn migrate() -> Result<()> {
        let foo = create_foo_value()?;

        let json = foo.to_json()?.to_string();
        println!("{}", json);

        let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));
        super::migrate::<_, Foo>(0, json.as_bytes(), store.clone())?;

        for entry in store.range(..) {
            let entry = entry?;
            println!("{:?} {:?}", entry.0, entry.1);
        }

        Ok(())
    }
}
