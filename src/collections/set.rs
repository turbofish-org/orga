use super::map::Map;
use crate::state::State;
use crate::store::DefaultBackingStore;
use crate::store::{Read, Store, Write};
use crate::Result;
use crate::encoding::Encode;
use std::hash::Hash;
use ed::*;

/// A set is literally identical to a hashmap in implementation, it's just the public API that is
/// different
/// this should just be a wrapper around hashmap
///
/// when things are added, they are hashed and placed in the index at that hash
/// when things are retrieved, they are hashed and the value at that index is returned if it exists
///
/// So the implementation of the set should reflect that
/// K should be u64 or u32
///
/// V should be the thing that you are looking to contain within the set 
///     in order to make sure that V can be placed in the set, V must be hashable?
/// apparently V should also be State<S>, but I'm not sure what that is exactly
/// and S should be Read
///
///
pub struct Set<T, S = DefaultBackingStore> {
    map: Map<T, T, S>,
}

// TODO: use derive(State) once it supports generic parameters
impl<T, S> State<S> for Set<T, S> 
where
    T: State<S> + Encode + Terminated + Eq + Hash,
    S: Write,
{
    type Encoding = ();

    fn create(store: Store<S>, _: ()) -> Result<Self> {
        Ok(Set {
            map: Map::create(store, ())?,
        })
    }
    
    fn flush(self) -> Result<()>
    where
        S: Write,
    {
        self.map.flush()?;
        Ok(())
    }
}

impl<T, S> Set<T, S>
where 
    T: State<S, Encoding = T> + Encode + Terminated + Eq + Hash + Copy,
    S: Read
{
    pub fn len(&self) -> u64 {42}

    pub fn is_empty(&self) -> bool {true}
    
    pub fn contains(&self, value: T) -> Result<bool> {
        match self.map.get(value)? {
            Some(..) => {
                Ok(true)
            },
            None => {
                Ok(false)
            }
        }
    }

}

impl<T, S> Set<T, S>
where
    T: State<S, Encoding = T> + Encode + Terminated + Eq + Hash + Copy,
    S: Write
{
    pub fn insert(&mut self, value: T) -> Result<()> {
        self.map.entry(value)?.or_insert(value)?;
        Ok(())
    }

    pub fn remove(&mut self, value: T) {
        self.map.remove(value);
    }
    /*
    pub fn union(&self, other: Self) -> Self {
        let store = Store::new(MapStore::new());
        Set::create(store.clone(), ()).unwrap()
    }

    pub fn difference(&self, other: Self) -> Self {
        let store = Store::new(MapStore::new());
        Set::create(store.clone(), ()).unwrap()
    }

    pub fn intersect(&self, other: Self) -> Self {
        let store = Store::new(MapStore::new());
        Set::create(store.clone(), ()).unwrap()
    }
    */
}

/*
impl<T: State<S>, S: Write> ops::Add<Set<T, S>> for Set<T, S> {
    type Output = Set<T, S>;

    fn add(&self, _rhs: Set<T, S>) -> Result<()> {Ok(())}
}

impl<T: State<S>, S: Write> ops::AddAssign<Set<T, S>> for Set<T, S> {
    type Output = Set<T, S>;

    fn add_assign(&self, _rhs: Set<T, S>) -> Result<()> {self}
}
impl<T: State<S>, S: Write> ops::Sub<Set<T, S>> for Set<T, S> {
    type Output = Set<T, S>;

    fn sub(&self, _rhs: Set<T, S>) -> Set<T, S> {self}
}

impl<T: State<S>, S:Write> ops::SubAssign<Set<T, S>> for Set<T, S> {
    type Output = Set<T, S>;

    fn sub_assign(&self, _rhs: Set<T, S>) -> Set<T, S> {self}
}
*/

mod tests {
    use super::*;
    use crate::store::MapStore;

    #[test]
    fn set_add() {
        let store = Store::new(MapStore::new());
        let mut set: Set<u32, MapStore> = Set::create(store.clone(), ()).unwrap();

        set.insert(42).unwrap();
        assert!(set.contains(42).unwrap());
    }
    
    #[test]
    fn deque_remove() {
        let store = Store::new(MapStore::new());
        let mut set: Set<u32, MapStore> = Set::create(store.clone(), ()).unwrap();

        set.insert(42).unwrap();
        set.remove(42);
        assert!(!set.contains(42).unwrap());
    }

    #[test]
    fn 
}
