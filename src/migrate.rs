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
    struct FooV0 {
        bar: u32,
        baz: Map<u32, u32>,
        beep: Map<u32, Map<bool, u8>>,
    }

    fn create_foo_v0_value() -> Result<Value> {
        let mut store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));

        let mut foo = FooV0::default();
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
    pub fn migrate_identity() -> Result<()> {
        let foo = create_foo_v0_value()?;
        let json = foo.to_json()?.to_string();

        let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));
        super::migrate::<_, FooV0>(0, json.as_bytes(), store.clone())?;

        let mut iter = store.range(..);
        let mut assert_next = |k: &[u8], v: &[u8]| {
            assert_eq!(
                iter.next().transpose().unwrap(),
                Some((k.to_vec(), v.to_vec()))
            )
        };
        assert_next(&[], &[0, 0, 0, 0]);
        assert_next(&[1, 0, 0, 0, 123], &[0, 0, 1, 200]);
        assert_next(&[1, 0, 0, 3, 21], &[0, 0, 0, 1]);
        assert_next(&[1, 0, 0, 3, 232], &[0, 0, 0, 2]);
        assert_next(&[1, 0, 0, 3, 233], &[0, 0, 0, 3]);
        assert_next(&[2, 0, 0, 0, 10], &[]);
        assert_next(&[2, 0, 0, 0, 10, 0], &[0]);
        assert_next(&[2, 0, 0, 0, 10, 1], &[1]);
        assert_next(&[2, 0, 0, 0, 20], &[]);
        assert_next(&[2, 0, 0, 0, 20, 0], &[1]);
        assert_next(&[2, 0, 0, 0, 20, 1], &[0]);
        assert_eq!(iter.next().transpose()?, None);

        Ok(())
    }

    #[derive(State, Encode, Decode, Describe, Default, Serialize, Deserialize)]
    #[state(version = 1, transform = "transform_foo")]
    struct FooV1 {
        bar: u64,
        baz: Map<u32, u32>,
        beep: Map<u32, Map<u8, u8>>,
    }

    fn transform_foo(version: u32, value: &mut crate::JsonValue) -> Result<()> {
        match version {
            0 => value["beep"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .for_each(|arr| {
                    arr.as_array_mut().unwrap()[1]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .for_each(|arr| {
                            let key = &mut arr.as_array_mut().unwrap()[0];
                            *key = serde_json::to_value(match key.as_bool().unwrap() {
                                false => 0,
                                true => 1,
                            })
                            .unwrap();
                        })
                }),
            1 => {}
            _ => {
                return Err(crate::Error::State(format!(
                    "Cannot upgrade from version {}",
                    version
                )))
            }
        };

        Ok(())
    }

    #[test]
    fn migrate_transform() -> Result<()> {
        let foo = create_foo_v0_value()?;
        let json = foo.to_json()?.to_string();

        let store = Store::new(DefaultBackingStore::MapStore(Shared::new(MapStore::new())));
        super::migrate::<_, FooV1>(0, json.as_bytes(), store.clone())?;

        let mut iter = store.range(..);
        let mut assert_next = |k: &[u8], v: &[u8]| {
            assert_eq!(
                iter.next().transpose().unwrap(),
                Some((k.to_vec(), v.to_vec()))
            )
        };
        assert_next(&[], &[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_next(&[1, 0, 0, 0, 123], &[0, 0, 1, 200]);
        assert_next(&[1, 0, 0, 3, 21], &[0, 0, 0, 1]);
        assert_next(&[1, 0, 0, 3, 232], &[0, 0, 0, 2]);
        assert_next(&[1, 0, 0, 3, 233], &[0, 0, 0, 3]);
        assert_next(&[2, 0, 0, 0, 10], &[]);
        assert_next(&[2, 0, 0, 0, 10, 0], &[0]);
        assert_next(&[2, 0, 0, 0, 10, 1], &[1]);
        assert_next(&[2, 0, 0, 0, 20], &[]);
        assert_next(&[2, 0, 0, 0, 20, 0], &[1]);
        assert_next(&[2, 0, 0, 0, 20, 1], &[0]);
        assert_eq!(iter.next().transpose()?, None);

        Ok(())
    }
}
