use std::{io::Write, iter::Peekable};

use crate::{
    describe::{Children, Describe, Value},
    store::{Iter, Store},
    Error, Result,
};

pub fn export<W: Write>(root: Value, out: &mut W) -> Result<()> {
    fn recurse<W: Write>(
        value: Value,
        iter: &mut Peekable<Iter<Store>>,
        out: &mut W,
    ) -> Result<()> {
        let desc = value.describe();
        match desc.children() {
            Children::Named(children) => {
                for child in children.iter() {
                    value
                        .child(&child.name)?
                        .map(|child| recurse(child, iter, out))
                        .transpose()?;
                }
            }
            Children::Dynamic(kv_desc) => {
                let store = value.store();
                // TODO: don't skip first byte after ABCI refactor
                let prefix = &store.prefix()[1..].to_vec();
                while let Some(Ok((key_bytes, value_bytes))) = iter.peek() {
                    if key_bytes < prefix {
                        panic!(
                            "Earlier data was not consumed (type={}, key={:?}, prefix={:?})\n ",
                            value.type_name(),
                            &key_bytes,
                            &prefix,
                        );
                    }
                    if !key_bytes.starts_with(prefix.as_slice()) {
                        break;
                    }

                    let key = kv_desc.key_desc().decode(&key_bytes[prefix.len()..])?;
                    let mut value = kv_desc.value_desc().decode(value_bytes.as_slice())?;

                    // TODO: remove this after ABCI refactor
                    let mut child_prefix = vec![0];
                    child_prefix.extend(key_bytes.as_slice());
                    value.attach(unsafe { store.with_prefix(child_prefix) })?;

                    out.write_all(
                        format!("[{},{}]\n", key.to_json()?, value.to_json()?).as_bytes(),
                    )?;

                    iter.next().transpose()?;
                    recurse(value, iter, out)?;
                }
                out.write_all("\n".as_bytes())?;
            }
            Children::None => {}
        }

        Ok(())
    }

    let json = root.to_json()?.to_string();
    out.write_all(json.as_bytes())?;
    out.write_all("\n".as_bytes())?;

    let mut iter = root.store().range(..).peekable();
    recurse(root, &mut iter, out)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{
        collections::Map,
        encoding::{Decode, Encode},
        state::State,
        store::{DefaultBackingStore, MapStore, Shared, Store, Write},
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
    fn export_test() -> Result<()> {
        let foo = create_foo_value()?;

        let mut json = Vec::new();
        export(foo, &mut json)?;

        println!("{}", String::from_utf8(json.clone()).unwrap());
        assert_eq!(
            String::from_utf8(json.clone()).unwrap(),
            "{\"bar\":0,\"baz\":{},\"beep\":{}}\n[123,456]\n[789,1]\n[1000,2]\n[1001,3]\n\n[10,{}]\n[false,0]\n[true,1]\n\n[20,{}]\n[false,1]\n[true,0]\n\n\n".to_string(),
        );

        Ok(())
    }
}
