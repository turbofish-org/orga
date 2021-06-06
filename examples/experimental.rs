use orga::state::*;
use orga::store::{MapStore, Store, Read};
use orga::collections::Map;
use orga::Result;

fn increment_entry(map: &mut Map<u64, u64>, n: u64) -> Result<()> {
    *map.entry(n)?.or_default()? += 1;
    Ok(())
}

fn main() {
  let store = Store::new(MapStore::new());
  let mut map = Map::create(store.clone(), ()).unwrap();

  let mut submap = map
      .entry(12).unwrap()
      .or_default().unwrap();
  increment_entry(&mut submap, 34).unwrap();

  let mut submap = map
      .entry(56).unwrap()
      .or_default().unwrap();
  increment_entry(&mut submap, 78).unwrap();
  increment_entry(&mut submap, 78).unwrap();
  increment_entry(&mut submap, 79).unwrap();

  let map_ref = &map;
  assert_eq!(
      *map_ref
          .get(12).unwrap().unwrap()
          .get(34).unwrap().unwrap(),
      1,
  );
  assert_eq!(
      *map_ref
          .get(56).unwrap().unwrap()
          .get(78).unwrap().unwrap(),
      2,
  );

  map
      .entry(56).unwrap()
      .remove().unwrap();

  map.flush().unwrap();
  
  for item in store.range(..) {
    println!("{:?}", item);
  }
}
