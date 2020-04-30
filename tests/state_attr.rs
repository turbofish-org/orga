use orga::{Store, MapStore, State, Value, state, Read};

#[state]
struct MyStruct<S: Store> {
    a: Value<u64>,
    c: MyStruct2
}

#[state]
struct MyStruct2<T: Store> {
    a: Value<u64>
}

#[test]
fn struct_state() {
    let mut store = MapStore::new();

    {
        let mut state: MyStruct<_> = store.as_mut().wrap().unwrap();

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
        store.get(&[1, 0]).unwrap(),
        Some(vec![0, 0, 0, 0, 0, 0, 0, 5])
    );
}
