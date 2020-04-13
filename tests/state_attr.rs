use orga::{Store, MapStore, WrapStore, Value, state, Read, Write};

#[state]
struct MyStruct {
    a: Value<u64>,
    b: Value<u32>,
    c: MyStruct2
}

#[state]
struct MyStruct2 {
    a: Value<u64>
}

#[test]
fn struct_state() {
    let mut store = MapStore::new();

    {
        let mut state = MyStruct::wrap_store(&mut store).unwrap();

        assert_eq!(state.a.get_or_default().unwrap(), 0);
        state.a.set(1234).unwrap();
        assert_eq!(state.a.get().unwrap(), 1234);

        assert_eq!(state.c.a.get_or_default().unwrap(), 0);
        state.c.a.set(5).unwrap();
        assert_eq!(state.c.a.get().unwrap(), 5);
    }

    assert_eq!(
        store.get(&[0]).unwrap(),
        Some(vec![0, 0, 0, 0, 0, 0, 4, 210])
    );
    assert_eq!(
        store.get(&[2, 0]).unwrap(),
        Some(vec![0, 0, 0, 0, 0, 0, 0, 5])
    );
}
