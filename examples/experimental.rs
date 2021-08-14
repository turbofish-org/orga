use ed::Encode;
use orga::collections::Map;
use orga::state::*;
use orga::store::{MapStore, Read, Store, Write};
use orga::Result;

#[derive(State)]
struct Foo {
    count: u32,
    map: Map<u32, Map<u32, u32>>,
}

fn increment_entry(map: &mut Map<u32, u32>, n: u32) -> Result<()> {
    *map.entry(n)?.or_default()? += 1;
    Ok(())
}

fn main() {
    let mut store = Store::new(MapStore::new());
    let mut foo = Foo::create(store.clone(), Default::default()).unwrap();

    foo.count += 42;

    let mut submap = foo.map.entry(12).unwrap().or_default().unwrap();
    increment_entry(&mut submap, 34).unwrap();

    let mut submap = foo.map.entry(56).unwrap().or_default().unwrap();
    increment_entry(&mut submap, 78).unwrap();
    increment_entry(&mut submap, 78).unwrap();
    increment_entry(&mut submap, 79).unwrap();

    let map_ref = &foo.map;
    assert_eq!(
        *map_ref.get(12).unwrap().unwrap().get(34).unwrap().unwrap(),
        1,
    );
    assert_eq!(
        *map_ref.get(56).unwrap().unwrap().get(78).unwrap().unwrap(),
        2,
    );

    foo.map.entry(56).unwrap().remove().unwrap();

    let data = foo.flush().unwrap();

    store.put(vec![], data.encode().unwrap()).unwrap();

    for item in store.range(..) {
        println!("{:?}", item);
    }
}
