//! A store backed by a partial in-memory map.
use std::collections::BTreeMap;

use super::Error;
use super::*;

/// A store backed by an in-memory map which may have missing values, and may be
/// joined with other partial map stores.
#[derive(Default)]
pub struct PartialMapStore {
    map: BTreeMap<Vec<u8>, (bool, Vec<u8>)>,
    right_edge: bool,
}

impl PartialMapStore {
    /// Creates a new empty partial map store.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new partial map store from an in-memory map.
    #[inline]
    pub fn from_map(map: BTreeMap<Vec<u8>, (bool, Vec<u8>)>, right_edge: bool) -> Self {
        Self { map, right_edge }
    }

    /// Joins two partial map stores.
    pub fn join(self, other: Self) -> Self {
        let mut map = self.map;
        for (k, (mut c, v)) in other.map {
            if let Some((c2, v2)) = map.get(&k) {
                c = c || *c2;
                if v != *v2 {
                    panic!("conflicting values for key {:?}", k);
                }
            }
            map.insert(k, (c, v));
        }
        Self {
            map,
            right_edge: self.right_edge || other.right_edge,
        }
    }
}

impl Read for PartialMapStore {
    #[inline]
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.get(key) {
            Some((_, v)) => Ok(Some(v.clone())),
            None => {
                let next = self.map.range(exclusive_range_starting_from(key)).next();

                if let Some((_, (c, _))) = next {
                    if *c {
                        return Ok(None);
                    }
                } else if self.right_edge {
                    return Ok(None);
                }

                Err(Error::GetUnknown(key.to_vec()).into())
            }
        }
    }

    // TODO: optimize by retaining previously used iterator(s) so we don't
    // have to recreate them each iteration (if it makes a difference)

    #[inline]
    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let next = self
            .map
            .range(exclusive_range_starting_from(key))
            .map(|(k, v)| (k.clone(), v.clone()))
            .next();

        if let Some((k, (c, v))) = next {
            if c {
                return Ok(Some((k, v)));
            }
        } else if self.right_edge {
            return Ok(None);
        }

        Err(Error::GetNextUnknown(key.to_vec()).into())
    }

    #[inline]
    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        let range = key.map_or((Bound::Unbounded, Bound::Unbounded), |key| {
            exclusive_range_ending_at(key)
        });
        let prev = self
            .map
            .range(range)
            .map(|(k, v)| (k.clone(), v.clone()))
            .next_back();

        let prev_key = if let Some((ref k, _)) = prev {
            k.clone()
        } else {
            vec![]
        };

        let next = self
            .map
            .range(exclusive_range_starting_from(&prev_key))
            .next();
        if let Some((_, (c, _))) = next {
            if *c {
                return Ok(prev.map(|(k, (_, v))| (k, v)));
            }
        } else if self.right_edge {
            return Ok(prev.map(|(k, (_, v))| (k, v)));
        }

        Err(Error::GetPrevUnknown(key.map(|k| k.to_vec())).into())
    }
}

/// Return range bounds which start from the given key (exclusive), with an
/// unbounded end.
fn exclusive_range_starting_from(start: &[u8]) -> (Bound<Vec<u8>>, Bound<Vec<u8>>) {
    (Bound::Excluded(start.to_vec()), Bound::Unbounded)
}

fn exclusive_range_ending_at(start: &[u8]) -> (Bound<Vec<u8>>, Bound<Vec<u8>>) {
    (Bound::Unbounded, Bound::Excluded(start.to_vec()))
}
